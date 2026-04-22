use std::{
    fs,
    panic::{self, AssertUnwindSafe},
    path::PathBuf,
    process::Command,
};

use c0mpiler::{
    ast::{Crate, Eatable},
    ir::layout::TargetDataLayout,
    irgen::IRGenerator,
    lexer::{Lexer, TokenBuffer},
    mir::lower::{LowerOptions, RV32Lowerer},
    semantics::analyzer::SemanticAnalyzer,
    utils::test::TestCaseInfo,
};

#[test]
fn my_asm() {
    let escape_list: [&str; 0] = [];
    run_test_cases_with_reimu(&escape_list, "testcases/asm", true);
}

#[test]
fn my_ir() {
    let escape_list: [&str; 0] = [];
    run_test_cases_with_reimu(&escape_list, "testcases/IR", true);
}

#[test]
fn ir_1_asm() {
    let escape_list: [&str; 0] = [];
    run_test_cases_with_reimu(&escape_list, "RCompiler-Testcases/IR-1", false);
}

fn require_reimu_path() -> String {
    std::env::var("REIMU_PATH")
        .unwrap_or("/home/color/workspace/Arch/REIMU/build/linux/x86_64/release/reimu".to_string())
}

fn run_test_cases_with_reimu(escape_list: &[&str], case_path: &str, stop_at_fault: bool) {
    let reimu_path = require_reimu_path();
    let path = PathBuf::from(case_path);
    let case_name = path.file_name().expect("invalid test case path");
    let infos_path = path.join("global.json");
    let infos: Vec<TestCaseInfo> =
        serde_json::from_str(fs::read_to_string(infos_path).unwrap().as_str()).unwrap();

    let mut total: usize = 0;
    let mut success: usize = 0;

    macro_rules! fault {
		($($t:tt)*) => {
			if stop_at_fault {
				panic!($($t)*);
			} else {
				println!($($t)*);
				println!();
				continue;
			}
		};
	}

    // Compile prelude.c to riscv32 assembly once.
    let temp_target_path = format!("target/tmp/{}/asm", case_name.display());
    fs::create_dir_all(&temp_target_path).unwrap();
    let prelude_asm = format!("{temp_target_path}/prelude.s");

    let prelude_compile = Command::new("clang")
        .args([
            "--target=riscv32-unknown-elf",
            "-S",
            "tests/prelude.c",
            "-O2",
            "-o",
            &prelude_asm,
        ])
        .output()
        .expect("Failed to compile prelude.c");

    if !prelude_compile.status.success() {
        let stderr = String::from_utf8_lossy(&prelude_compile.stderr);
        panic!("Failed to compile prelude.c to riscv32 assembly:\n{stderr}");
    }

    for x in infos {
        let name = x.name;
        if escape_list.contains(&name.as_str()) {
            println!("{name} skiped!");
            continue;
        }
        total += 1;

        let src_path = path.join(format!("src/{name}/{name}.rx"));
        let in_path = path.join(format!("src/{name}/{name}.in"));
        let out_path = path.join(format!("src/{name}/{name}.out"));

        let src = fs::read_to_string(&src_path).unwrap();
        let should_pass = x.compileexitcode == 0;

        let timer = std::time::Instant::now();
        let sub_timer = std::time::Instant::now();

        let parser_result = panic::catch_unwind(|| -> Result<Crate, String> {
            let lexer = Lexer::new(&src);
            let buffer = TokenBuffer::new(lexer).map_err(|e| format!("{:?}", e))?;
            let mut iter = buffer.iter();
            let krate = Crate::eat(&mut iter).map_err(|e| format!("{:?}", e))?;
            Ok(krate)
        })
        .unwrap_or_else(|_| panic!("{name} caused panic during parsing!"));

        let krate = match parser_result {
            Ok(krate) => krate,
            Err(e) => {
                if should_pass {
                    fault!("{name} parse failed, expect pass!\n{e}");
                } else {
                    println!("{name} passed (parse failed as expected)!");
                    success += 1;
                    continue;
                }
            }
        };

        let parse_time = sub_timer.elapsed();
        let sub_timer = std::time::Instant::now();

        let semantic_result = panic::catch_unwind(|| -> Result<_, String> {
            let (analyzer, result) = SemanticAnalyzer::visit(&krate);
            result.map_err(|e| format!("{:?}", e))?;
            Ok(analyzer)
        });

        let analyzer = match semantic_result {
            Ok(Ok(analyzer)) => analyzer,
            Ok(Err(e)) => {
                if should_pass {
                    fault!("{name} semantic check failed, expect pass!\n{e}");
                } else {
                    println!("{name} passed (semantic check failed as expected)!");
                    success += 1;
                    continue;
                }
            }
            Err(_) => {
                fault!("{name} caused panic during semantic check!");
            }
        };

        if !should_pass {
            fault!("{name} semantic check passed, expect fail!");
        }

        let semantic_time = sub_timer.elapsed();
        let mut ir_time: std::time::Duration = std::time::Duration::new(0, 0);
        let mut asm_time: std::time::Duration = std::time::Duration::new(0, 0);

        let asm = match panic::catch_unwind(AssertUnwindSafe(|| {
            let sub_timer = std::time::Instant::now();

            let mut generator = IRGenerator::new(&analyzer, TargetDataLayout::rv32());
            generator.visit(&krate);
            generator.opt_mem2reg();

            ir_time = sub_timer.elapsed();

            let sub_timer = std::time::Instant::now();

            let module = generator.module();

            let mut lowerer = RV32Lowerer::with_options(LowerOptions {
                lower_function_bodies: true,
                need_branch_relaxation: true,
                optimize_fallthroughs: true,
                optimize_peephole: true,
            });
            let machine_module = lowerer.lower_module(&module).expect("MIR lowering failed");

            asm_time = sub_timer.elapsed();

            machine_module.to_string()
        })) {
            Ok(asm) => asm,
            Err(_) => {
                fault!("{name} caused panic during asm generation!");
            }
        };

        let asm_file = format!("{temp_target_path}/{name}.s");
        fs::write(&asm_file, &asm).unwrap();

        let compile_time = timer.elapsed();
        let timer = std::time::Instant::now();

        let in_arg = if in_path.exists() {
            format!("-i={}", in_path.display())
        } else {
            String::new()
        };

        let out_arg = if out_path.exists() {
            format!("-a={}", out_path.display())
        } else {
            String::new()
        };

        let mut reimu_args = vec![
            format!("-f={},{}", prelude_asm, asm_file),
            "--oj-mode".to_string(),
            "-s=1M".to_string(),
        ];

        if !in_arg.is_empty() {
            reimu_args.push(in_arg);
        }
        if !out_arg.is_empty() {
            reimu_args.push(out_arg);
        }

        let reimu_result = Command::new(&reimu_path).args(&reimu_args).output();

        let reimu_output = match reimu_result {
            Ok(output) => output,
            Err(e) => {
                fault!("{name} failed to execute reimu: {e}");
            }
        };

        if !reimu_output.status.success() {
            let stderr = String::from_utf8_lossy(&reimu_output.stderr);
            let stdout = String::from_utf8_lossy(&reimu_output.stdout);
            fault!("{name} reimu execution failed:\nstdout:\n{stdout}\nstderr:\n{stderr}");
        }

        let running_time = timer.elapsed();

        println!(
            "{name} passed! Compile time: {compile_time:.2?} (Parse: {parse_time:.2?}, Semantic: {semantic_time:.2?}, IR: {ir_time:.2?}, ASM: {asm_time:.2?}), Running time: {running_time:.2?}"
        );
        success += 1;
    }

    println!("Test Result: {}/{}", success, total);
    if success < total {
        panic!();
    }
}
