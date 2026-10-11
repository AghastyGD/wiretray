use anyhow::{Context, Result, bail};

use super::{DEFAULT_AP_INTERFACE, command_runner::CommandRunner, runtime_state::IptablesRule};

const IP_FORWARD_KEY: &str = "net.ipv4.ip_forward";
const RULE_COMMENT: &str = "wiretray";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallState {
    pub ip_forwarding_was_enabled: bool,
    pub iptables_rules: Vec<IptablesRule>,
}

pub async fn enable_network_sharing<R>(runner: &R, upstream: &str) -> Result<FirewallState>
where
    R: CommandRunner,
{
    let forwarding_was_enabled = read_ip_forwarding(runner).await?;

    if !forwarding_was_enabled {
        set_ip_forwarding(runner, true).await?;
    }

    let mut state = FirewallState {
        ip_forwarding_was_enabled: forwarding_was_enabled,
        iptables_rules: Vec::new(),
    };

    for rule in sharing_rules(upstream) {
        if let Err(error) = apply_rule(runner, &rule).await {
            let startup_error = error.context(format!(
                "Failed to apply Wiretray firewall rule in {}/{}",
                rule.table, rule.chain
            ));

            return match remove_network_sharing(runner, &state).await {
                Ok(()) => Err(startup_error.context("Firewall changes were rolled back")),
                Err(cleanup_error) => Err(anyhow::anyhow!(
                    "{startup_error:#}; firewall rollback also failed: {cleanup_error:#}"
                )),
            };
        }

        state.iptables_rules.push(rule);
    }

    Ok(state)
}

pub async fn remove_network_sharing<R>(runner: &R, state: &FirewallState) -> Result<()>
where
    R: CommandRunner,
{
    let mut errors = Vec::new();

    for rule in state.iptables_rules.iter().rev() {
        if let Err(error) = remove_rule(runner, rule).await {
            errors.push(format!(
                "failed to remove rule from {}/{}: {error:#}",
                rule.table, rule.chain
            ));
        }
    }

    if !state.ip_forwarding_was_enabled
        && let Err(error) = set_ip_forwarding(runner, false).await
    {
        errors.push(format!("failed to restore IPv4 forwarding: {error:#}"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!(errors.join("; "))
    }
}

pub(super) fn sharing_rules(upstream: &str) -> Vec<IptablesRule> {
    vec![
        IptablesRule {
            table: "nat".into(),
            chain: "POSTROUTING".into(),
            args: string_args(&[
                "-o",
                upstream,
                "-m",
                "comment",
                "--comment",
                RULE_COMMENT,
                "-j",
                "MASQUERADE",
            ]),
        },
        IptablesRule {
            table: "filter".into(),
            chain: "FORWARD".into(),
            args: string_args(&[
                "-i",
                DEFAULT_AP_INTERFACE,
                "-o",
                upstream,
                "-m",
                "comment",
                "--comment",
                RULE_COMMENT,
                "-j",
                "ACCEPT",
            ]),
        },
        IptablesRule {
            table: "filter".into(),
            chain: "FORWARD".into(),
            args: string_args(&[
                "-i",
                upstream,
                "-o",
                DEFAULT_AP_INTERFACE,
                "-m",
                "conntrack",
                "--ctstate",
                "RELATED,ESTABLISHED",
                "-m",
                "comment",
                "--comment",
                RULE_COMMENT,
                "-j",
                "ACCEPT",
            ]),
        },
    ]
}

async fn read_ip_forwarding<R>(runner: &R) -> Result<bool>
where
    R: CommandRunner,
{
    let output = runner
        .run("sysctl", &["-n", IP_FORWARD_KEY])
        .await
        .context("Failed to read IPv4 forwarding state")?;

    match output.stdout.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        value => bail!("Unexpected IPv4 forwarding value: {value:?}"),
    }
}

async fn set_ip_forwarding<R>(runner: &R, enabled: bool) -> Result<()>
where
    R: CommandRunner,
{
    let value = if enabled {
        "net.ipv4.ip_forward=1"
    } else {
        "net.ipv4.ip_forward=0"
    };

    runner
        .run("sysctl", &["-w", value])
        .await
        .with_context(|| format!("Failed to set IPv4 forwarding to {enabled}"))?;

    Ok(())
}

async fn apply_rule<R>(runner: &R, rule: &IptablesRule) -> Result<()>
where
    R: CommandRunner,
{
    run_rule_command(runner, "-A", rule).await
}

async fn remove_rule<R>(runner: &R, rule: &IptablesRule) -> Result<()>
where
    R: CommandRunner,
{
    if rule_exists(runner, rule).await? {
        run_rule_command(runner, "-D", rule).await?;
    }

    Ok(())
}

async fn rule_exists<R>(runner: &R, rule: &IptablesRule) -> Result<bool>
where
    R: CommandRunner,
{
    let mut arguments = vec!["-t", rule.table.as_str(), "-C", rule.chain.as_str()];
    arguments.extend(rule.args.iter().map(String::as_str));

    let output = runner.output("iptables", &arguments).await?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => bail!(
            "Failed to check rule in {}/{}: {}",
            rule.table,
            rule.chain,
            output.stderr.trim()
        ),
    }
}

async fn run_rule_command<R>(runner: &R, operation: &str, rule: &IptablesRule) -> Result<()>
where
    R: CommandRunner,
{
    let mut arguments = vec!["-t", rule.table.as_str(), operation, rule.chain.as_str()];
    arguments.extend(rule.args.iter().map(String::as_str));

    runner.run("iptables", &arguments).await?;

    Ok(())
}

fn string_args(arguments: &[&str]) -> Vec<String> {
    arguments
        .iter()
        .map(|argument| (*argument).into())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque, os::unix::process::ExitStatusExt, process::ExitStatus, sync::Mutex,
    };

    use anyhow::Result;

    use super::*;
    use crate::services::concurrent_ap::command_runner::CommandOutput;

    struct ScriptedRunner {
        outputs: Mutex<VecDeque<(i32, String)>>,
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl ScriptedRunner {
        fn new(outputs: impl IntoIterator<Item = (i32, &'static str)>) -> Self {
            Self {
                outputs: Mutex::new(
                    outputs
                        .into_iter()
                        .map(|(code, stdout)| (code, stdout.to_string()))
                        .collect(),
                ),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandRunner for ScriptedRunner {
        fn command_exists(&self, _command: &str) -> bool {
            true
        }

        async fn output(&self, command: &str, args: &[&str]) -> Result<CommandOutput> {
            self.calls.lock().unwrap().push((
                command.to_string(),
                args.iter().map(|argument| argument.to_string()).collect(),
            ));
            let (code, stdout) = self
                .outputs
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected command execution");

            Ok(CommandOutput {
                status: ExitStatus::from_raw(code << 8),
                stdout,
                stderr: if code == 0 {
                    String::new()
                } else {
                    "scripted failure".into()
                },
            })
        }
    }

    #[tokio::test]
    async fn enables_forwarding_and_adds_only_wiretray_rules() {
        let runner = ScriptedRunner::new([(0, "0\n"), (0, ""), (0, ""), (0, ""), (0, "")]);

        let state = enable_network_sharing(&runner, "enp2s0").await.unwrap();

        assert!(!state.ip_forwarding_was_enabled);
        assert_eq!(state.iptables_rules.len(), 3);
        assert!(
            state
                .iptables_rules
                .iter()
                .all(|rule| rule.args.iter().any(|argument| argument == RULE_COMMENT))
        );
        assert!(
            runner
                .calls()
                .iter()
                .all(|(_, args)| !args.contains(&"-F".to_string()))
        );
    }

    #[tokio::test]
    async fn rolls_back_exact_rules_when_an_addition_fails() {
        let runner = ScriptedRunner::new([
            (0, "0\n"),
            (0, ""),
            (0, ""),
            (1, ""),
            (0, ""),
            (0, ""),
            (0, ""),
        ]);

        let error = enable_network_sharing(&runner, "wlp3s0")
            .await
            .unwrap_err()
            .to_string();
        let calls = runner.calls();

        assert!(error.contains("Firewall changes were rolled back"));
        assert!(calls.iter().any(|(command, args)| {
            command == "iptables" && args.windows(2).any(|pair| pair == ["-D", "POSTROUTING"])
        }));
        assert_eq!(
            calls.last(),
            Some(&(
                "sysctl".into(),
                vec!["-w".to_string(), "net.ipv4.ip_forward=0".to_string()]
            ))
        );
    }

    #[tokio::test]
    async fn preserves_previously_enabled_forwarding() {
        let runner = ScriptedRunner::new([
            (0, "1\n"),
            (0, ""),
            (0, ""),
            (0, ""),
            (0, ""),
            (0, ""),
            (0, ""),
            (0, ""),
            (0, ""),
            (0, ""),
        ]);

        let state = enable_network_sharing(&runner, "enp2s0").await.unwrap();
        remove_network_sharing(&runner, &state).await.unwrap();

        assert!(state.ip_forwarding_was_enabled);
        assert_eq!(
            runner
                .calls()
                .iter()
                .filter(|(command, _)| command == "sysctl")
                .count(),
            1
        );
    }
}
