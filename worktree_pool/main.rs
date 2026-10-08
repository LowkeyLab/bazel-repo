fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(worktree_pool::cli::run(
        &std::env::args_os().collect::<Vec<_>>(),
    ))
}
