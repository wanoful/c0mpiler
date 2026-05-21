mod common;

use std::{fs, panic::{self, AssertUnwindSafe}, path::PathBuf, process::Command};

use c0mpiler::{
    ir::layout::TargetDataLayout,
    irgen::IRGenerator,
    mir::lower::{LowerOptions, RV32Lowerer, RV64Lowerer},
    semantics::analyzer::SemanticAnalyzer,
};

use common::{compare_output, load_test_infos, with_frontend};

macro_rules! fault {
    ($stop:expr, $($t:tt)*) => {
        if $stop { panic!($($t)*); } else { println!($($t)*); println!(); continue; }
    };
}

#[test]
fn my_asm() { run_rv32_mir("testcases/asm", &[]); }
#[test]
fn my_ir() { run_rv32_mir("testcases/IR", &[]); }
#[test]
fn ir_1_asm() { run_rv32_mir("RCompiler-Testcases/IR-1", &[]); }

fn run_rv32_mir(case_path: &str, escape_list: &[&str]) {
    let reimu_path = std::env::var("REIMU_PATH")
        .unwrap_or("/home/color/workspace/Arch/REIMU/build/linux/x86_64/release/reimu".to_string());
    let infos = load_test_infos(case_path);
    let temp = format!("target/tmp/{}/asm", PathBuf::from(case_path).file_name().unwrap().display());
    fs::create_dir_all(&temp).unwrap();

    let prelude_asm = format!("{temp}/prelude.s");
    let out = Command::new("clang")
        .args(["--target=riscv32-unknown-elf", "-S", "tests/prelude.c", "-O2", "-o", &prelude_asm])
        .output().expect("Failed to compile prelude.c");
    assert!(out.status.success(), "prelude failed:\n{}", String::from_utf8_lossy(&out.stderr));

    let (mut total, mut success) = (0, 0);
    for x in &infos {
        let name = &x.name;
        if escape_list.contains(&name.as_str()) { println!("{name} skiped!"); continue; }
        total += 1;

        let src = fs::read_to_string(format!("{case_path}/src/{name}/{name}.rx")).unwrap();
        let timer = std::time::Instant::now();

        let asm = match with_frontend(&src, x.compileexitcode == 0, |analyzer, krate| {
            gen_rv32_mir(analyzer, krate)
        }) {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => { println!("{name} passed ({e})!"); success += 1; continue; }
            Err(e) => { println!("{name} passed ({e})!"); success += 1; continue; }
        };

        fs::write(format!("{temp}/{name}.s"), &asm).unwrap();
        let in_path = format!("{case_path}/src/{name}/{name}.in");
        let out_path = format!("{case_path}/src/{name}/{name}.out");
        let in_arg = if PathBuf::from(&in_path).exists() { format!("-i={in_path}") } else { String::new() };
        let out_arg = if PathBuf::from(&out_path).exists() { format!("-a={out_path}") } else { String::new() };
        let mut args = vec![format!("-f={prelude_asm},{temp}/{name}.s"), "--oj-mode".into(), "-s=1M".into()];
        if !in_arg.is_empty() { args.push(in_arg); }
        if !out_arg.is_empty() { args.push(out_arg); }

        match Command::new(&reimu_path).args(&args).output() {
            Ok(o) if o.status.success() => {}
            Ok(o) => { fault!(true, "{name} reimu failed:\n{}", String::from_utf8_lossy(&o.stderr)); }
            Err(e) => { fault!(true, "{name} reimu exec error: {e}"); }
        }
        println!("{name} passed! {:.2?}", timer.elapsed());
        success += 1;
    }
    println!("Test Result: {success}/{total}");
    assert_eq!(success, total);
}

fn gen_rv32_mir(analyzer: &SemanticAnalyzer, krate: &c0mpiler::ast::Crate) -> Result<String, String> {
    panic::catch_unwind(AssertUnwindSafe(|| {
        let mut g = IRGenerator::new(analyzer, TargetDataLayout::rv32());
        g.visit(krate);
        g.opt_all();
        let mut lowerer = RV32Lowerer::with_options(LowerOptions {
            lower_function_bodies: true, need_branch_relaxation: true,
            optimize_fallthroughs: true, optimize_peephole: true,
        });
        lowerer.lower_module(&g.module()).unwrap().to_string()
    })).map_err(|_| "panic during asm generation".to_string())
}

#[test]
fn my_rv64_asm() { run_rv64_qemu("testcases/asm", &[]); }
#[test]
fn my_rv64_ir() { run_rv64_qemu("testcases/IR", &[]); }

fn run_rv64_qemu(case_path: &str, escape_list: &[&str]) {
    let qemu_path = std::env::var("QEMU_RV64_PATH")
        .unwrap_or("/home/color/workspace/os/qemu-10.2.1/build/qemu-riscv64".to_string());
    let infos = load_test_infos(case_path);
    let temp = format!("target/tmp/{}/rv64_asm", PathBuf::from(case_path).file_name().unwrap().display());
    fs::create_dir_all(&temp).unwrap();

    let prelude_o = format!("{temp}/prelude.o");
    let out = Command::new("riscv64-linux-gnu-gcc")
        .args(["-O2", "-c", "tests/prelude.c", "-o", &prelude_o])
        .output().expect("Failed to compile prelude.c");
    assert!(out.status.success(), "prelude failed:\n{}", String::from_utf8_lossy(&out.stderr));

    let (mut total, mut success) = (0, 0);
    for x in &infos {
        let name = &x.name;
        if escape_list.contains(&name.as_str()) { println!("{name} skiped!"); continue; }
        total += 1;

        let src = fs::read_to_string(format!("{case_path}/src/{name}/{name}.rx")).unwrap();
        let timer = std::time::Instant::now();

        let asm = match with_frontend(&src, x.compileexitcode == 0, |analyzer, krate| {
            gen_rv64_mir(analyzer, krate)
        }) {
            Ok(Ok(a)) => a,
            Ok(Err(e)) => { println!("{name} passed ({e})!"); success += 1; continue; }
            Err(e) => { println!("{name} passed ({e})!"); success += 1; continue; }
        };

        fs::write(format!("{temp}/{name}.s"), &asm).unwrap();
        let in_path = format!("{case_path}/src/{name}/{name}.in");
        let out_path = format!("{case_path}/src/{name}/{name}.out");

        let obj = format!("{temp}/{name}.o");
        let asm_out = Command::new("riscv64-linux-gnu-gcc")
            .args(["-c", &format!("{temp}/{name}.s"), "-o", &obj]).output();
        match asm_out {
            Ok(o) if o.status.success() => {}
            Ok(o) => { fault!(true, "{name} asm failed:\n{}", String::from_utf8_lossy(&o.stderr)); }
            Err(e) => { fault!(true, "{name} assembler error: {e}"); }
        }

        let elf = format!("{temp}/{name}.elf");
        let ld_out = Command::new("riscv64-linux-gnu-gcc")
            .args(["-static", &prelude_o, &obj, "-o", &elf]).output();
        match ld_out {
            Ok(o) if o.status.success() => {}
            Ok(o) => { fault!(true, "{name} link failed:\n{}", String::from_utf8_lossy(&o.stderr)); }
            Err(e) => { fault!(true, "{name} linker error: {e}"); }
        }

        let input_data = if PathBuf::from(&in_path).exists() { fs::read(&in_path).unwrap() } else { Vec::new() };
        let qemu = Command::new(&qemu_path).arg(&elf)
            .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped()).spawn();
        let mut child = match qemu {
            Ok(c) => c,
            Err(e) => { fault!(true, "{name} qemu exec error: {e}"); }
        };
        if !input_data.is_empty() {
            use std::io::Write;
            child.stdin.take().unwrap().write_all(&input_data).unwrap();
        }
        let output = child.wait_with_output().unwrap();
        if !output.status.success() {
            fault!(true, "{name} qemu failed:\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        }
        match compare_output(&output.stdout, &PathBuf::from(&out_path)) {
            Ok(()) => {}
            Err(e) => { fault!(true, "{name} {e}"); }
        }
        println!("{name} passed! {:.2?}", timer.elapsed());
        success += 1;
    }
    println!("Test Result: {success}/{total}");
    assert_eq!(success, total);
}

fn gen_rv64_mir(analyzer: &SemanticAnalyzer, krate: &c0mpiler::ast::Crate) -> Result<String, String> {
    panic::catch_unwind(AssertUnwindSafe(|| {
        let mut g = IRGenerator::new(analyzer, TargetDataLayout::rv64());
        g.visit(krate);
        g.opt_all();
        let mut lowerer = RV64Lowerer::with_options(LowerOptions {
            lower_function_bodies: true, need_branch_relaxation: true,
            optimize_fallthroughs: true, optimize_peephole: true,
        });
        lowerer.lower_module(&g.module()).unwrap().to_string()
    })).map_err(|_| "panic during asm generation".to_string())
}
