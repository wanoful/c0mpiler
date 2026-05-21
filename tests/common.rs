use std::{fs, panic::{self, AssertUnwindSafe}, path::Path};

use c0mpiler::{
    ast::{Crate, Eatable},
    lexer::{Lexer, TokenBuffer},
    semantics::analyzer::SemanticAnalyzer,
    utils::test::TestCaseInfo,
};

pub fn load_test_infos(case_path: &str) -> Vec<TestCaseInfo> {
    let infos_path = Path::new(case_path).join("global.json");
    serde_json::from_str(&fs::read_to_string(infos_path).unwrap()).unwrap()
}

pub fn with_frontend<T>(
    src: &str,
    should_pass: bool,
    f: impl FnOnce(&SemanticAnalyzer, &Crate) -> T,
) -> Result<T, String> {
    let parser_result = panic::catch_unwind(|| -> Result<Crate, String> {
        let lexer = Lexer::new(src);
        let buffer = TokenBuffer::new(lexer).map_err(|e| format!("{:?}", e))?;
        let mut iter = buffer.iter();
        Crate::eat(&mut iter).map_err(|e| format!("{:?}", e))
    });

    let krate = match parser_result {
        Ok(Ok(krate)) => krate,
        Ok(Err(e)) if !should_pass => return Err(format!("parse failed as expected: {e}")),
        Ok(Err(e)) => return Err(format!("parse failed, expect pass: {e}")),
        Err(_) => return Err("panic during parsing".to_string()),
    };

    let result = panic::catch_unwind(AssertUnwindSafe(|| -> Result<T, String> {
        let (analyzer, sem_result) = SemanticAnalyzer::visit(&krate);
        sem_result.map_err(|e| format!("{:?}", e))?;
        if !should_pass {
            return Err("semantic check passed, expect fail".to_string());
        }
        Ok(f(&analyzer, &krate))
    }));

    match result {
        Ok(Ok(t)) => Ok(t),
        Ok(Err(e)) if !should_pass => Err(format!("semantic check failed as expected: {e}")),
        Ok(Err(e)) => Err(format!("semantic check failed, expect pass: {e}")),
        Err(_) => Err("panic during semantic check".to_string()),
    }
}

pub fn compare_output(actual: &[u8], out_path: &Path) -> Result<(), String> {
    let expected = if out_path.exists() {
        fs::read(out_path).unwrap()
    } else {
        Vec::new()
    };
    if actual.trim_ascii_end() != expected.trim_ascii_end() {
        let actual_str = String::from_utf8_lossy(actual);
        let expected_str = String::from_utf8_lossy(&expected);
        return Err(format!(
            "output mismatch!\nExpected:\n{}\nActual:\n{}",
            expected_str, actual_str
        ));
    }
    Ok(())
}
