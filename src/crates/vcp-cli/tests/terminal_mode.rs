// SPDX-License-Identifier: Apache-2.0
use clap::Parser;
use vcp_cli::args::Cli;

#[test]
fn terminal_dispatch_preserves_redirected_and_automation_protocols() {
    let workspace = tempfile::tempdir().unwrap();
    for flags in [
        vec![],
        vec!["--format", "jsonl"],
        vec!["--non-interactive"],
        vec!["--control-stdin"],
    ] {
        let mut argv = vec![
            "vcp",
            "--workspace",
            workspace.path().to_str().unwrap(),
            "run",
            "inspect sources",
            "--budget-usd",
            "0.01",
        ];
        argv.extend(flags.iter().copied());
        let cli = Cli::try_parse_from(argv).unwrap().validate(None).unwrap();
        assert_eq!(cli.interactive_terminal(true, true, true), flags.is_empty());
        for (stdin, stdout, stderr) in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            assert!(!cli.interactive_terminal(stdin, stdout, stderr));
        }
    }
}
