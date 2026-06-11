mod common;

use std::{fs, mem, panic::{self, AssertUnwindSafe}, path::PathBuf, process::{Command, Stdio}, str::FromStr};

use c0mpiler::{ast::Crate, ir::layout::TargetDataLayout, irgen::IRGenerator, semantics::analyzer::SemanticAnalyzer};

use common::{compare_output, load_test_infos, with_frontend};

macro_rules! fault {
    ($stop:expr, $($t:tt)*) => {
        if $stop { panic!($($t)*); } else { println!($($t)*); println!(); continue; }
    };
}

#[test]
fn my_semantic() {
    run_semantic("testcases/semantics", &["copy_trait1","copy_trait2","copy_trait3","operator1","autoderef1","item_order1","item_order2","type1"]);
}
#[test]
fn semantic_1() {
    run_semantic("RCompiler-Testcases/semantic-1", &["misc3","misc4","misc14"]);
}

fn run_semantic(case_path: &str, escape_list: &[&str]) {
    let infos = load_test_infos(case_path);
    let (mut total, mut success) = (0, 0);
    for x in &infos {
        let name = &x.name;
        if escape_list.contains(&name.as_str()) { println!("{name} skiped!"); continue; }
        total += 1;
        let src = fs::read_to_string(format!("{case_path}/src/{name}/{name}.rx")).unwrap();
        match with_frontend(&src, x.compileexitcode == 0, |_, _| ()) {
            Ok(_) | Err(_) => { println!("{name} passed!"); success += 1; }
        }
    }
    println!("Test Result: {success}/{total}");
    assert_eq!(success, total);
}

#[test]
fn my_ir() {
    if let Some(p) = option_env!("REIMU_PATH") { run_ir_reimu(p, "testcases/IR", &[]); }
    else { run_ir_native("testcases/IR", &[], true); }
}
#[test]
fn ir_1() {
    if let Some(p) = option_env!("REIMU_PATH") { run_ir_reimu(p, "RCompiler-Testcases/IR-1", &[]); }
    else { run_ir_native("RCompiler-Testcases/IR-1", &[], true); }
}

fn run_ir_native(case_path: &str, escape_list: &[&str], dry_run: bool) {
    let infos = load_test_infos(case_path);
    let temp = format!("target/tmp/{}", PathBuf::from_str(case_path).unwrap().file_name().unwrap().display());
    fs::create_dir_all(&temp).unwrap();
    let (mut total, mut success) = (0, 0);

    for x in &infos {
        let name = &x.name;
        if escape_list.contains(&name.as_str()) { println!("{name} skiped!"); continue; }
        total += 1;

        let src = fs::read_to_string(format!("{case_path}/src/{name}/{name}.rx")).unwrap();
        let in_path = format!("{case_path}/src/{name}/{name}.in");
        let out_path = format!("{case_path}/src/{name}/{name}.out");

        let result = with_frontend(&src, x.compileexitcode == 0, |analyzer, krate| {
            gen_ir_native(analyzer, krate)
        });

        let ir = match result {
            Ok(Ok(ir)) if x.compileexitcode == 0 => ir,
            Err(e) if x.compileexitcode != 0 && !e.starts_with("__UNEXPECTED_PASS__") => {
                println!("{name} passed ({e})!"); success += 1; continue;
            }
            Ok(Err(e)) | Err(e) => {
                fault!(true, "{name} unexpected result (expect_pass={}): {e}",
                    x.compileexitcode == 0);
            }
            _ => unreachable!(),
        };

        let ir_file = format!("{temp}/{name}.ll");
        fs::write(&ir_file, &ir).unwrap();

        let out = Command::new("clang").args([&ir_file, "builtin/builtin.c", "-o", &format!("{temp}/{name}")]).output();
        match out {
            Ok(o) if o.status.success() => {}
            Ok(o) => { fault!(true, "{name} compile failed:\n{}", String::from_utf8_lossy(&o.stderr)); }
            Err(e) => { fault!(true, "{name} clang error: {e}"); }
        }

        if !dry_run {
            let input = if PathBuf::from(&in_path).exists() { fs::read(&in_path).unwrap() } else { Vec::new() };
            let child = Command::new(format!("{temp}/{name}"))
                .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
            let mut child = match child {
                Ok(c) => c,
                Err(e) => { fault!(true, "{name} exec error: {e}"); }
            };
            if !input.is_empty() {
                use std::io::Write;
                child.stdin.take().unwrap().write_all(&input).unwrap();
            }
            let output = child.wait_with_output().unwrap();
            if !output.status.success() {
                fault!(true, "{name} run failed:\n{}", String::from_utf8_lossy(&output.stderr));
            }
            match compare_output(&output.stdout, &PathBuf::from(&out_path)) {
                Ok(()) => {}
                Err(e) => { fault!(true, "{name} {e}"); }
            }
        }
        println!("{name} passed!");
        success += 1;
    }
    println!("Test Result: {success}/{total}");
    assert_eq!(success, total);
}

fn gen_ir_native(analyzer: &SemanticAnalyzer, krate: &Crate) -> Result<String, String> {
    panic::catch_unwind(AssertUnwindSafe(|| {
        const PTR_SIZE: u32 = mem::size_of::<usize>() as u32;
        let mut g = IRGenerator::new(analyzer, TargetDataLayout { pointer_size: PTR_SIZE, pointer_align: PTR_SIZE });
        g.visit(krate);
        g.opt_all();
        g.print()
    })).map_err(|_| "panic during IR generation".to_string())
}

fn run_ir_reimu(reimu_path: &str, case_path: &str, escape_list: &[&str]) {
    let infos = load_test_infos(case_path);
    let temp = format!("target/tmp/{}", PathBuf::from_str(case_path).unwrap().file_name().unwrap().display());
    fs::create_dir_all(&temp).unwrap();

    let prelude_asm = format!("{temp}/prelude.s");
    let out = Command::new("clang")
        .args(["--target=riscv32-unknown-elf", "-S", "builtin/builtin.c", "-O2", "-o", &prelude_asm])
        .output().expect("Failed to compile builtin.c");
    assert!(out.status.success(), "prelude failed:\n{}", String::from_utf8_lossy(&out.stderr));

    let (mut total, mut success) = (0, 0);
    for x in &infos {
        let name = &x.name;
        if escape_list.contains(&name.as_str()) { println!("{name} skiped!"); continue; }
        total += 1;

        let src = fs::read_to_string(format!("{case_path}/src/{name}/{name}.rx")).unwrap();
        let in_path = format!("{case_path}/src/{name}/{name}.in");
        let out_path = format!("{case_path}/src/{name}/{name}.out");

        let result = with_frontend(&src, x.compileexitcode == 0, |analyzer, krate| {
            gen_ir_rv32(analyzer, krate)
        });

        let ir = match result {
            Ok(Ok(ir)) if x.compileexitcode == 0 => ir,
            Err(e) if x.compileexitcode != 0 && !e.starts_with("__UNEXPECTED_PASS__") => {
                println!("{name} passed ({e})!"); success += 1; continue;
            }
            Ok(Err(e)) | Err(e) => {
                fault!(true, "{name} unexpected result (expect_pass={}): {e}",
                    x.compileexitcode == 0);
            }
            _ => unreachable!(),
        };

        let ir_file = format!("{temp}/{name}.ll");
        fs::write(&ir_file, &ir).unwrap();

        let ir_asm = format!("{temp}/{name}.s");
        let out = Command::new("clang")
            .args(["--target=riscv32-unknown-elf", "-S", &ir_file, "-o", &ir_asm]).output();
        match out {
            Ok(o) if o.status.success() => {}
            Ok(o) => { fault!(true, "{name} clang compile failed:\n{}", String::from_utf8_lossy(&o.stderr)); }
            Err(e) => { fault!(true, "{name} clang error: {e}"); }
        }

        let in_arg = if PathBuf::from(&in_path).exists() { format!("-i={in_path}") } else { String::new() };
        let out_arg = if PathBuf::from(&out_path).exists() { format!("-a={out_path}") } else { String::new() };
        let mut args = vec![format!("-f={prelude_asm},{ir_asm}"), "--oj-mode".into(), "-s=1M".into()];
        if !in_arg.is_empty() { args.push(in_arg); }
        if !out_arg.is_empty() { args.push(out_arg); }

        match Command::new(reimu_path).args(&args).output() {
            Ok(o) if o.status.success() => {}
            Ok(o) => { fault!(true, "{name} reimu failed:\n{}", String::from_utf8_lossy(&o.stderr)); }
            Err(e) => { fault!(true, "{name} reimu error: {e}"); }
        }
        println!("{name} passed!");
        success += 1;
    }
    println!("Test Result: {success}/{total}");
    assert_eq!(success, total);
}

fn gen_ir_rv32(analyzer: &SemanticAnalyzer, krate: &Crate) -> Result<String, String> {
    panic::catch_unwind(AssertUnwindSafe(|| {
        let mut g = IRGenerator::new(analyzer, TargetDataLayout::rv32());
        g.visit(krate);
        g.print()
    })).map_err(|_| "panic during IR generation".to_string())
}
