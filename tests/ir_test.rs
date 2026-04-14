use std::{
    fs, mem,
    panic::{self, AssertUnwindSafe},
    path::PathBuf,
    process::{Command, Stdio},
    str::FromStr,
};

use c0mpiler::{
    ast::{Crate, Eatable},
    ir::layout::TargetDataLayout,
    irgen::IRGenerator,
    lexer::{Lexer, TokenBuffer},
    semantics::analyzer::SemanticAnalyzer,
    utils::test::TestCaseInfo,
};

#[test]
fn my_semantic() {
    let escape_list = [
        "copy_trait1",
        "copy_trait2",
        "copy_trait3", // 不清楚 Copy Trait 要实现到哪一步
        "operator1",   // TODO: &1 == &1,
        "autoderef1",  // 这个点在 IR 的处理非常麻烦
        "item_order1",
        "item_order2",
        "type1",
    ];
    let case_path = "testcases/semantics";

    run_test_cases(&escape_list, case_path, true, true);
}

#[test]
fn semantic_1() {
    let escape_list = ["misc3", "misc4", "misc14"];
    let case_path = "RCompiler-Testcases/semantic-1";

    run_test_cases(&escape_list, case_path, true, true);
}

#[test]
fn my_ir() {
    let escape_list = [];
    let case_path = "testcases/IR";

    if let Some(reimu_path) = option_env!("REIMU_PATH") {
        println!("Using reimu at path: {}", reimu_path);
        run_test_cases_with_reimu(reimu_path, &escape_list, case_path, true);
    } else {
        run_test_cases(&escape_list, case_path, true, false);
    }
}

#[test]
fn ir_1() {
    let escape_list = [];
    let case_path = "RCompiler-Testcases/IR-1";

    if let Some(reimu_path) = option_env!("REIMU_PATH") {
        println!("Using reimu at path: {}", reimu_path);
        run_test_cases_with_reimu(reimu_path, &escape_list, case_path, true);
    } else {
        run_test_cases(&escape_list, case_path, true, false);
    }
}

fn run_test_cases(
    escape_list: &[&'static str],
    case_path: &'static str,
    stop_at_fault: bool,
    dry_run: bool,
) {
    let path = PathBuf::from_str(case_path).unwrap();
    let case_name = path.file_name().unwrap();
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

        // Parse and semantic check
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

        // If semantic check passed but should fail
        if !should_pass {
            fault!("{name} semantic check passed, expect fail!");
        }

        // Generate IR
        let ir = match panic::catch_unwind(AssertUnwindSafe(|| {
            const PTR_SIZE: u32 = mem::size_of::<usize>() as u32;
            let mut generator = IRGenerator::new(
                &analyzer,
                TargetDataLayout {
                    pointer_size: PTR_SIZE,
                    pointer_align: PTR_SIZE,
                },
            );
            generator.visit(&krate);
            generator.print()
        })) {
            Ok(ir) => ir,
            Err(_) => {
                fault!("{name} caused panic during IR generation!");
            }
        };

        // Write IR to temporary file
        let temp_target_path = format!("target/tmp/{}", case_name.display());
        let ir_file = format!("{temp_target_path}/{name}.ll");
        fs::create_dir_all(&temp_target_path).unwrap();
        fs::write(&ir_file, &ir).unwrap();

        // Compile with clang
        let compile_result = Command::new("clang")
            .args([
                &ir_file,
                "tests/prelude.c",
                "-o",
                &format!("{temp_target_path}/{name}"),
            ])
            .output();

        let compile_output = match compile_result {
            Ok(output) => output,
            Err(e) => {
                fault!("{name} failed to execute clang: {e}");
            }
        };

        if !compile_output.status.success() {
            let stderr = String::from_utf8_lossy(&compile_output.stderr);
            fault!("{name} compilation failed:\n{stderr}");
        }

        if !dry_run {
            // Run the compiled program
            let input_data = if in_path.exists() {
                fs::read(&in_path).unwrap()
            } else {
                Vec::new()
            };

            let run_result = Command::new(format!("{temp_target_path}/{name}"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn();

            let mut child = match run_result {
                Ok(child) => child,
                Err(e) => {
                    fault!("{name} failed to execute program: {e}");
                }
            };

            // Write input to stdin
            if !input_data.is_empty() {
                use std::io::Write;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(&input_data).unwrap();
                }
            }

            let output = match child.wait_with_output() {
                Ok(output) => output,
                Err(e) => {
                    fault!("{name} failed to wait for program: {e}");
                }
            };

            // Check if program exited successfully
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                fault!("{name} program execution failed:\n{stderr}");
            }

            // Read expected output
            let expected_output = if out_path.exists() {
                fs::read(&out_path).unwrap()
            } else {
                Vec::new()
            };

            let actual_output = output.stdout;

            // Compare outputs
            if actual_output.trim_ascii_end() != expected_output.trim_ascii_end() {
                let actual_str = String::from_utf8_lossy(&actual_output);
                let expected_str = String::from_utf8_lossy(&expected_output);
                fault!(
                    "{name} output mismatch!\nExpected:\n{}\nActual:\n{}",
                    expected_str,
                    actual_str
                );
            }
        }

        println!("{name} passed!");
        success += 1;
    }

    println!("Test Result: {}/{}", success, total);
    if success < total {
        panic!();
    }
}

fn run_test_cases_with_reimu(
    reimu_path: &'static str,
    escape_list: &[&'static str],
    case_path: &'static str,
    stop_at_fault: bool,
) {
    let path = PathBuf::from_str(case_path).unwrap();
    let case_name = path.file_name().unwrap();
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

    // Compile prelude.c to riscv32 assembly once
    let temp_target_path = format!("target/tmp/{}", case_name.display());
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

        // Parse and semantic check
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

        // If semantic check passed but should fail
        if !should_pass {
            fault!("{name} semantic check passed, expect fail!");
        }

        // Generate IR
        let ir = match panic::catch_unwind(AssertUnwindSafe(|| {
            let mut generator = IRGenerator::new(&analyzer, TargetDataLayout::rv32());
            generator.visit(&krate);
            generator.print()
        })) {
            Ok(ir) => ir,
            Err(_) => {
                fault!("{name} caused panic during IR generation!");
            }
        };

        // Write IR to temporary file
        let ir_file = format!("{temp_target_path}/{name}.ll");
        fs::write(&ir_file, &ir).unwrap();

        // Compile IR to riscv32 assembly
        let ir_asm = format!("{temp_target_path}/{name}.s");
        let ir_compile = Command::new("clang")
            .args([
                "--target=riscv32-unknown-elf",
                "-S",
                &ir_file,
                "-o",
                &ir_asm,
            ])
            .output();

        let ir_compile_output = match ir_compile {
            Ok(output) => output,
            Err(e) => {
                fault!("{name} failed to execute clang for IR: {e}");
            }
        };

        if !ir_compile_output.status.success() {
            let stderr = String::from_utf8_lossy(&ir_compile_output.stderr);
            fault!("{name} IR compilation to riscv32 failed:\n{stderr}");
        }

        // Build reimu arguments
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
            format!("-f={},{}", prelude_asm, ir_asm),
            "--oj-mode".to_string(),
            "-s=1M".to_string(),
        ];

        if !in_arg.is_empty() {
            reimu_args.push(in_arg);
        }
        if !out_arg.is_empty() {
            reimu_args.push(out_arg);
        }

        let reimu_result = Command::new(reimu_path).args(&reimu_args).output();

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

        println!("{name} passed!");
        success += 1;
    }

    println!("Test Result: {}/{}", success, total);
    if success < total {
        panic!();
    }
}
