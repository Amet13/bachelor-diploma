//! `proofback`: map reviewer feedback on a PDF to the exact line of your LaTeX
//! source, track what you did about it, and send the answers back.

mod commands;
mod extract;
mod git;
mod output;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "proofback", version, about = "Every margin note, back in your source.", long_about = None)]
pub struct Cli {
    /// Project directory (default: search upwards from the current directory).
    #[arg(long, global = true, value_name = "DIR")]
    pub project: Option<PathBuf>,
    /// Colour output.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,
    /// Print less.
    #[arg(long, short, global = true)]
    pub quiet: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create .proofback/ in the project directory.
    Init {
        /// Main LaTeX file (guessed when omitted).
        #[arg(long)]
        main: Option<String>,
    },
    /// Import a review: an annotated PDF.
    Import(ImportArgs),
    /// List items.
    List(ListArgs),
    /// Show one item with source context.
    Show {
        /// Item id or unique prefix.
        id: String,
        #[arg(long)]
        json: bool,
        /// Lines of source context around the target.
        #[arg(long, default_value_t = 3)]
        context: u32,
    },
    /// Set an item's status and add a reply.
    Resolve {
        id: String,
        /// open, done, wontfix, discuss or duplicate.
        #[arg(long)]
        status: Option<String>,
        /// Your reply to the reviewer.
        #[arg(long)]
        reply: Option<String>,
        /// Mark done although the anchored text is unchanged (a deliberate decision).
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print a unified diff for replace, delete and insert items.
    Patch {
        /// Item ids (default: every open, exactly located edit item).
        ids: Vec<String>,
    },
    /// Apply the edits to the source files.
    Apply {
        ids: Vec<String>,
        /// Show what would change without writing.
        #[arg(long)]
        dry_run: bool,
        /// Also mark applied items as done.
        #[arg(long)]
        resolve: bool,
    },
    /// Re-anchor items to the current source.
    Sync {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Write the reviewer's PDF back with a reply on every answered note.
    Answer {
        /// Output PDF.
        #[arg(short, long)]
        output: PathBuf,
        /// Round to answer (default: the most recent PDF round).
        #[arg(long)]
        round: Option<String>,
        /// The PDF the reviewer annotated (default: the file recorded at import).
        #[arg(long)]
        pdf: Option<PathBuf>,
    },
    /// Write a response letter.
    Letter {
        #[arg(long, value_enum, default_value_t = LetterFormat::Markdown)]
        format: LetterFormat,
        #[arg(long)]
        round: Option<String>,
        #[arg(long, default_value = "Response to the review")]
        title: String,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Insert comments for items into the source (`% proofback pb_…:` or `\todo{}`).
    Inline {
        ids: Vec<String>,
        /// Use \todo{} from the todonotes package instead of a comment.
        #[arg(long)]
        todo: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Write SARIF 2.1.0 for GitHub code scanning.
    Sarif {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Counts by round, status, file and reviewer.
    Stats {
        #[arg(long)]
        json: bool,
    },
    /// Check the project setup.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Print the JSON Schema of round files.
    Schema,
    /// Internal: parse a PDF in an isolated process.
    #[command(name = "__extract", hide = true)]
    Extract { args: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LetterFormat {
    Markdown,
    Latex,
}

#[derive(Debug, Args)]
pub struct ImportArgs {
    /// Annotated PDF.
    pub file: PathBuf,
    /// SyncTeX file of the build the reviewer read (default: found next to the PDF or in the build directory).
    #[arg(long)]
    pub synctex: Option<PathBuf>,
    /// Round name (default: date plus file name).
    #[arg(long)]
    pub round: Option<String>,
    /// Anchor against the sources at this git commit, then carry anchors forward.
    #[arg(long, value_name = "COMMIT")]
    pub at: Option<String>,
    /// Replace reviewer names with "Reviewer N".
    #[arg(long)]
    pub anonymize_authors: bool,
    /// Ignore SyncTeX even if present (text-only mode).
    #[arg(long)]
    pub no_synctex: bool,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args, Default)]
pub struct ListArgs {
    #[arg(long)]
    pub round: Option<String>,
    /// Filter by status (repeatable).
    #[arg(long)]
    pub status: Vec<String>,
    /// Filter by source file (prefix match).
    #[arg(long)]
    pub file: Option<String>,
    /// Filter by kind (repeatable).
    #[arg(long)]
    pub kind: Vec<String>,
    #[arg(long, conflicts_with = "md")]
    pub json: bool,
    /// GitHub task list.
    #[arg(long)]
    pub md: bool,
    /// Exit with code 1 when open items remain.
    #[arg(long)]
    pub fail_on_open: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if let Command::Extract { args } = &cli.command {
        return ExitCode::from(extract::child_main(args) as u8);
    }
    match commands::run(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("proofback: {e}");
            ExitCode::from(e.exit_code() as u8)
        }
    }
}
