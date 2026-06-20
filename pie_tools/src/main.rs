use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod cooker;
mod pak;

use cooker::cook_assets;
use pak::PakFile;

#[derive(Parser)]
#[command(name = "pie_tools", about = "Pie Engine asset tools")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Cook assets from a source directory into a .pak file
    Cook {
        /// Input assets directory
        #[arg(long, default_value = "assets")]
        input: PathBuf,
        /// Output directory for the cooked pak
        #[arg(long, default_value = "build")]
        output: PathBuf,
    },
    /// Export: cook assets and build the runtime binary
    Export {
        /// Input assets directory
        #[arg(long, default_value = "assets")]
        input: PathBuf,
        /// Output directory for the build artifacts
        #[arg(long, default_value = "build")]
        output: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::Cook { input, output } => {
            if let Err(error) = run_cook(&input, &output) {
                eprintln!("pie_tools: cook failed: {error}");
                return ExitCode::FAILURE;
            }
        }
        Commands::Export { input, output } => {
            if let Err(error) = run_cook(&input, &output) {
                eprintln!("pie_tools: cook failed: {error}");
                return ExitCode::FAILURE;
            }
            if let Err(error) = run_build(&output) {
                eprintln!("pie_tools: build failed: {error}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

fn run_cook(input: &std::path::Path, output: &std::path::Path) -> Result<(), String> {
    println!(
        "pie_tools: cooking assets from {} to {}",
        input.display(),
        output.display()
    );

    let pak = cook_assets(input).map_err(|error| error.to_string())?;

    std::fs::create_dir_all(output).map_err(|error| {
        format!(
            "failed to create output directory {}: {error}",
            output.display()
        )
    })?;

    let pak_path = output.join("assets.pak");
    PakFile::write_to_path(&pak, &pak_path).map_err(|error| {
        format!(
            "failed to write pak to {}: {error}",
            pak_path.display()
        )
    })?;

    println!(
        "pie_tools: cooked {} assets to {}",
        pak.assets.len(),
        pak_path.display()
    );

    Ok(())
}

fn run_build(output: &std::path::Path) -> Result<(), String> {
    println!("pie_tools: building pie_runtime release binary");

    let status = std::process::Command::new("cargo")
        .args(["build", "--release", "-p", "pie_runtime"])
        .status()
        .map_err(|error| format!("failed to spawn cargo build: {error}"))?;

    if !status.success() {
        return Err(format!(
            "cargo build exited with status: {}",
            status.code().unwrap_or(-1)
        ));
    }

    // Copy the built binary to the output directory.
    let target_dir = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "target".to_string());
    let binary_name = if cfg!(windows) {
        "pie_runtime.exe"
    } else {
        "pie_runtime"
    };
    let binary_src = PathBuf::from(&target_dir).join("release").join(binary_name);
    let binary_dst = output.join(binary_name);

    if binary_src.exists() {
        std::fs::copy(&binary_src, &binary_dst).map_err(|error| {
            format!(
                "failed to copy binary from {} to {}: {error}",
                binary_src.display(),
                binary_dst.display()
            )
        })?;
        println!(
            "pie_tools: runtime binary at {}",
            binary_dst.display()
        );
    } else {
        eprintln!(
            "pie_tools: warning: built binary not found at {}",
            binary_src.display()
        );
    }

    println!("pie_tools: export complete — output at {}", output.display());
    Ok(())
}
