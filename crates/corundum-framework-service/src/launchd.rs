//! Rendering a macOS LaunchAgent, user level only.
//!
//! A LaunchAgent starts with the user's session and lives in their own
//! `~/Library/LaunchAgents`, so it is theirs to remove — the macOS counterpart of a systemd
//! user unit.

use std::path::{Path, PathBuf};

use corundum_framework_platform::package::{PackageKind, PackageLayout};

use crate::scope::{InstallContext, ServiceSpec};

/// The label a LaunchAgent is registered under.
///
/// A LaunchAgent label is a global namespace on the machine: a generic label such as
/// `sapphire-agent` would collide with anything else that claims the same one, so the label
/// carries the publisher's domain, `sapphire-turtle.net`, the namespace corundum's apps use
/// too: `net.sapphire-turtle.<app>`.
pub fn label(app_name: &str) -> String {
    format!("net.sapphire-turtle.{app_name}")
}

/// Where a LaunchAgent is installed: `~/Library/LaunchAgents/<label>.plist`.
///
/// The file is named by the label, so `launchctl` finds it by the label's reverse-DNS path.
pub fn agent_path(app_name: &str, home: &Path) -> PathBuf {
    labelled_agent_path(&label(app_name), home)
}

/// Where an agent registered under `label` is installed: `~/Library/LaunchAgents/<label>.plist`.
///
/// For an agent under a label this crate does not build, such as an app's
/// [`ServiceSpec::legacy_labels`].
pub fn labelled_agent_path(label: &str, home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
        .join(format!("{label}.plist"))
}

/// A legacy agent's plist under the current [`label`]: the same program, arguments and
/// description, registered under the new name. `from_label` is the label the plist carries.
pub fn relabel(plist: &str, from_label: &str, app_name: &str) -> String {
    plist.replace(
        &format!("<string>{}</string>", escape(from_label)),
        &format!("<string>{}</string>", escape(&label(app_name))),
    )
}

/// Escape a string for inclusion as XML character data.
///
/// A single pass: ampersands are escaped first, then the angle brackets. Only the three
/// entities that make text unparseable: quotes are legal unescaped in text nodes, where
/// every dynamic value lands, and the template's own attribute values are static. An
/// entity form already present in the input (& typed literally) is itself escaped,
/// the way XML escaping works.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() + 12);
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Make a string safe inside an XML comment.
///
/// A double hyphen is forbidden in comments by the XML grammar, so a description like
/// `a -- b` would make the file unparseable; a trailing hyphen would too, so the
/// trailing space keeps the `-->` closer from reading as one.
fn comment_safe(text: &str) -> String {
    text.replace("--", "- -").trim_end().to_owned()
}

/// Turn a [`ServiceSpec`] and a resolved [`InstallContext`] into a LaunchAgent plist.
///
/// The agent runs one program with arguments: the plist's `ProgramArguments`, which launchd
/// execs directly, with no shell in between — so each argument becomes its own `<string>`
/// and a space inside an argument needs no quoting at all. `RunAtLoad` starts the agent
/// when the session loads it and `KeepAlive`/`SuccessfulExit=false` restarts it on failure,
/// the counterparts of a user unit's activation and `Restart=on-failure`; the description
/// lands in an XML comment, the place a human looks and the one place the XML
/// grammar (unlike the plist) has room for it.
pub fn render_launch_agent(spec: &ServiceSpec, ctx: &InstallContext) -> String {
    render_agent(spec, None, &ctx.exe.display().to_string())
}

/// The plist a macOS package carries in `Contents/Library/LaunchAgents/<label>.plist`, for
/// SMAppService to register.
///
/// The same agent as [`render_launch_agent`], with the program given relative to the
/// bundle: `BundleProgram` is `Contents/MacOS/<exe_name>`, which launchd resolves against
/// wherever the `.app` is now, so moving the app does not break the agent. `exe_name` is
/// the first `ProgramArguments` string, the `argv[0]` the program sees.
pub fn bundled_agent_plist(spec: &ServiceSpec, exe_name: &str) -> String {
    render_agent(spec, Some(&format!("Contents/MacOS/{exe_name}")), exe_name)
}

/// The agent plist both renderers share: `bundle_program` is the `BundleProgram` key's value
/// when there is one, `program` the first `ProgramArguments` string.
fn render_agent(spec: &ServiceSpec, bundle_program: Option<&str>, program: &str) -> String {
    let mut agent = String::new();
    agent.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    agent.push_str("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ");
    agent.push_str("\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
    agent.push_str("<plist version=\"1.0\">\n<dict>\n");
    agent.push_str("  <key>Label</key>\n");
    agent.push_str(&format!(
        "  <string>{}</string>\n",
        escape(&label(spec.app_name))
    ));
    if let Some(bundle_program) = bundle_program {
        agent.push_str("  <key>BundleProgram</key>\n");
        agent.push_str(&format!("  <string>{}</string>\n", escape(bundle_program)));
    }
    agent.push_str("  <key>ProgramArguments</key>\n  <array>\n");
    agent.push_str(&format!("    <string>{}</string>\n", escape(program)));
    for arg in &spec.args {
        agent.push_str(&format!("    <string>{}</string>\n", escape(arg)));
    }
    agent.push_str("  </array>\n");
    agent.push_str("  <key>RunAtLoad</key>\n  <true/>\n");
    agent.push_str("  <key>KeepAlive</key>\n  <dict>\n");
    agent.push_str("    <key>SuccessfulExit</key>\n    <false/>\n");
    agent.push_str("  </dict>\n");
    agent.push_str("  <key>ThrottleInterval</key>\n  <integer>5</integer>\n");
    // The description for `launchctl list`, as an XML comment: launchd reads the plist
    // through an XML parser, so `--` runs (illegal inside a comment) become `- -`.
    agent.push_str(&format!("  <!-- {} -->\n", comment_safe(&spec.description)));
    agent.push_str("</dict>\n</plist>\n");
    agent
}

/// How a macOS install registers the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MacBackend {
    /// Through SMAppService, with the plist the `.app` carries: the registration goes when
    /// the app is deleted, and the person manages it in System Settings › Login Items.
    AppService {
        /// The plist's file name in `Contents/Library/LaunchAgents/`, as SMAppService asks
        /// for it: `<label>.plist`.
        plist_name: String,
    },
    /// Through a LaunchAgent file in `~/Library/LaunchAgents/`, loaded with `launchctl`.
    AgentFile,
}

/// Which [`MacBackend`] an install uses: SMAppService when the app runs from a macOS
/// package whose `Contents/Library/LaunchAgents/<label>.plist` exists, the LaunchAgent file
/// otherwise.
///
/// A pure rule: `layout` is [`package::current`](corundum_framework_platform::package::current)'s
/// answer and `plist_present` says whether a file exists, so every case is testable on any
/// machine. A package without the plist (one packaged before it carried one) falls back to
/// the file, which works anywhere.
pub fn mac_backend(
    layout: Option<&PackageLayout>,
    plist_present: impl Fn(&Path) -> bool,
    app_name: &str,
) -> MacBackend {
    let plist_name = format!("{}.plist", label(app_name));
    match layout {
        Some(layout)
            if layout.kind == PackageKind::MacBundle
                && plist_present(&bundled_agent_path(&layout.root, &plist_name)) =>
        {
            MacBackend::AppService { plist_name }
        }
        _ => MacBackend::AgentFile,
    }
}

/// Where a `.app` carries an agent's plist: `<app>/Contents/Library/LaunchAgents/<name>`.
pub fn bundled_agent_path(app: &Path, plist_name: &str) -> PathBuf {
    app.join("Contents/Library/LaunchAgents").join(plist_name)
}

/// The program an agent runs: the first `ProgramArguments` string; `None` when there is none.
pub fn program_of(plist: &str) -> Option<PathBuf> {
    let (_, after) = plist.split_once("<key>ProgramArguments</key>")?;
    let (_, after) = after.split_once("<string>")?;
    let (program, _) = after.split_once("</string>")?;
    Some(PathBuf::from(unescape(program)))
}

/// Whether the output of `launchctl print` says the agent is running.
///
/// The line `state = not running` also contains `running`, so the whole value is compared.
pub fn is_running(print: &str) -> bool {
    print
        .lines()
        .any(|line| line.trim().strip_prefix("state = ") == Some("running"))
}

/// Undo [`escape`] for text read back out of a plist.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_is_the_first_program_argument() {
        let golden = include_str!("../tests/golden/launchagent.plist");
        assert_eq!(
            program_of(golden),
            Some(PathBuf::from("/usr/bin/sapphire-agent"))
        );
        assert_eq!(program_of("<plist></plist>"), None);
        assert_eq!(
            program_of(&render_launch_agent(
                &spec(),
                &InstallContext {
                    exe: PathBuf::from("/Applications/A & B/agent"),
                    ..ctx()
                }
            )),
            Some(PathBuf::from("/Applications/A & B/agent"))
        );
    }

    #[test]
    fn only_a_running_state_is_running() {
        for (print, expected) in [
            ("net.x = {\n\tstate = running\n\tpid = 4\n}\n", true),
            ("net.x = {\n\tstate = not running\n}\n", false),
            ("", false),
        ] {
            assert_eq!(is_running(print), expected, "{print}");
        }
    }

    #[test]
    fn an_agent_lives_under_the_users_launch_agents() {
        assert_eq!(
            agent_path("sapphire-agent", Path::new("/Users/alice")),
            PathBuf::from(
                "/Users/alice/Library/LaunchAgents/net.sapphire-turtle.sapphire-agent.plist"
            )
        );
        assert_eq!(
            labelled_agent_path("org.old.agent", Path::new("/Users/alice")),
            PathBuf::from("/Users/alice/Library/LaunchAgents/org.old.agent.plist")
        );
    }

    #[test]
    fn a_legacy_agent_is_relabelled_and_keeps_its_program() {
        let legacy =
            render_launch_agent(&spec(), &ctx()).replace(&label("sapphire-agent"), "org.old.agent");
        assert!(legacy.contains("org.old.agent"));
        assert_eq!(
            relabel(&legacy, "org.old.agent", "sapphire-agent"),
            render_launch_agent(&spec(), &ctx())
        );
    }

    #[test]
    fn entity_text_escapes_to_a_single_pass() {
        // A raw ampersand is escaped; entity forms already in the input are escaped whole.
        assert_eq!(escape("a & b"), "a &amp; b");
        assert_eq!(escape("&"), "&amp;");
        assert_eq!(escape("<&>"), "&lt;&amp;&gt;");
    }

    fn spec() -> ServiceSpec {
        ServiceSpec {
            app_name: "sapphire-agent",
            description: "Sapphire agent server".into(),
            args: vec!["server".into(), "run".into()],
            legacy_labels: vec![],
            post_install: None,
        }
    }

    fn ctx() -> InstallContext {
        InstallContext {
            unit_path: PathBuf::from("/dev/null"),
            exe: PathBuf::from("/usr/bin/sapphire-agent"),
        }
    }

    fn bundle(kind: PackageKind) -> PackageLayout {
        PackageLayout {
            kind,
            root: PathBuf::from("/Applications/Demo.app"),
            bin_dir: PathBuf::from("/Applications/Demo.app/Contents/MacOS"),
        }
    }

    #[test]
    fn the_backend_is_smappservice_only_in_a_bundle_carrying_the_plist() {
        let carried = PathBuf::from(
            "/Applications/Demo.app/Contents/Library/LaunchAgents/net.sapphire-turtle.demo.plist",
        );
        let present = |path: &Path| path == carried;
        assert_eq!(mac_backend(None, present, "demo"), MacBackend::AgentFile);
        assert_eq!(
            mac_backend(Some(&bundle(PackageKind::MacBundle)), present, "demo"),
            MacBackend::AppService {
                plist_name: "net.sapphire-turtle.demo.plist".to_owned()
            }
        );
        assert_eq!(
            mac_backend(Some(&bundle(PackageKind::MacBundle)), |_| false, "demo"),
            MacBackend::AgentFile
        );
        assert_eq!(
            mac_backend(Some(&bundle(PackageKind::AppImage)), present, "demo"),
            MacBackend::AgentFile
        );
    }

    #[test]
    fn a_double_hyphen_in_a_description_is_comment_safe() {
        let spec = ServiceSpec {
            description: "Notes -- ledger".into(),
            args: vec![],
            ..spec()
        };
        let rendered = render_launch_agent(&spec, &ctx());
        assert!(rendered.contains("<!-- Notes - - ledger -->"), "{rendered}");
        assert!(
            !rendered.contains("<!-- Notes -- ledger -->"),
            "a `--` run inside a comment is illegal XML: {rendered}"
        );
    }
}
