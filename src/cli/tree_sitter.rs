use std::path::PathBuf;

use clap::Args;

#[derive(Args)]
pub struct TreeSitterArgs {
    /// Output directory (default: current directory)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Also invoke `tree-sitter generate` to produce parser.c
    #[arg(long)]
    pub generate: bool,
}

pub fn cmd_tree_sitter(args: TreeSitterArgs) {
    let grammar = trident::syntax::grammar::trident_grammar();
    let json = grammar.to_json();

    let out_dir = args.output.unwrap_or_else(|| PathBuf::from("."));
    let out_path = out_dir.join("grammar.json");

    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        eprintln!("error: cannot create '{}': {}", out_dir.display(), e);
        std::process::exit(1);
    }

    if let Err(e) = std::fs::write(&out_path, &json) {
        eprintln!("error: cannot write '{}': {}", out_path.display(), e);
        std::process::exit(1);
    }
    eprintln!("Wrote {}", out_path.display());

    if args.generate {
        let parent = out_dir.parent().unwrap_or(&out_dir);
        let status = std::process::Command::new("tree-sitter")
            .arg("generate")
            .arg(&out_path)
            .current_dir(parent)
            .status();
        match status {
            Ok(s) if s.success() => eprintln!("parser.c generated"),
            Ok(s) => {
                eprintln!("tree-sitter generate exited with {}", s);
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("error: cannot run tree-sitter: {}", e);
                eprintln!("hint: cargo install tree-sitter-cli");
                std::process::exit(1);
            }
        }
    }
}
