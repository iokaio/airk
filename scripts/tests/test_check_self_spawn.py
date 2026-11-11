# SPDX-License-Identifier: Apache-2.0
import importlib.util
from pathlib import Path
import tempfile
import textwrap
import unittest

SPEC = importlib.util.spec_from_file_location(
    "airk_check_self_spawn", Path(__file__).parents[1] / "check-self-spawn.py"
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class SelfSpawnTests(unittest.TestCase):
    def tree(self, files):
        root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        for name, text in files.items():
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(textwrap.dedent(text), encoding="utf-8")
        return root

    def test_production_self_spawns_and_role_starts_are_found_but_tests_are_not(self):
        root = self.tree({
            "crates/demo/src/lib.rs": """
                mod owner;
                #[cfg(test)]
                mod fixtures;
                #[cfg(test)]
                #[path = "named_tests.rs"]
                mod named;
                fn direct() -> std::io::Result<()> {
                    // std::env::current_exe() in a comment is not a start.
                    let image = std::env::current_exe()?;
                    std::process::Command::new(image).spawn()?;
                    Ok(())
                }
                fn label() -> &'static str { "current_exe() in a string" }
                #[cfg(test)]
                mod tests {
                    fn inline() { std::process::Command::new(std::env::current_exe().unwrap()); }
                }
            """,
            "crates/demo/src/owner.rs": """
                fn start(image: &std::path::Path) {
                    std::process::Command::new(image).arg("project-worker");
                }
                fn names_only() -> &'static str { "source-worker" }
            """,
            "crates/demo/src/fixtures.rs": """
                fn fixture() { std::process::Command::new(std::env::current_exe().unwrap()); }
            """,
            "crates/demo/src/named_tests.rs": """
                fn fixture() { std::process::Command::new(std::env::current_exe().unwrap()); }
            """,
        })
        found, test_files = MODULE.scan(root)
        self.assertEqual(
            set(found),
            {("crates/demo/src/lib.rs", "direct"), ("crates/demo/src/owner.rs", "start")},
        )
        self.assertEqual(
            {path.name for path in test_files}, {"fixtures.rs", "named_tests.rs"}
        )

    def test_an_unreviewed_site_or_a_stale_roster_entry_fails(self):
        root = self.tree({
            "crates/demo/src/lib.rs": """
                fn direct() { std::process::Command::new(std::env::current_exe().unwrap()); }
            """,
        })
        self.assertEqual(MODULE.main(["--root", str(root)]), 1)

    def test_former_false_negatives_are_found(self):
        root = self.tree({
            "crates/demo/src/lib.rs": """
                const ROLE: &str = "project-worker";
                // The image arrives as a parameter: only the start roster sees it.
                fn launch(image: &std::path::Path) { std::process::Command::new(image); }
                // A `;` inside the signature no longer ends the function early.
                fn sized(bytes: [u8; 4]) -> std::io::Result<()> {
                    std::process::Command::new(std::env::current_exe()?);
                    Ok(())
                }
                // A raw byte string with a backslash does not hide the code after it.
                fn raw() -> std::io::Result<()> {
                    let _ = br"C:\\";
                    std::process::Command::new(std::env::current_exe()?);
                    Ok(())
                }
                fn native(image: &[u16]) { unsafe { CreateProcessW(image.as_ptr()); } }
                fn detached(image: &std::path::Path) { spawn_detached(image, &[]); }
                fn run_limited(spec: &Spec) {}
                struct ProviderCommand { program: std::path::PathBuf }
                impl ProviderCommand { fn validate(&self) {} }
            """,
        })
        found, _, starts = MODULE.scan(root, with_starts=True)
        self.assertEqual(
            set(found),
            {
                ("crates/demo/src/lib.rs", MODULE.MODULE_ITEM),
                ("crates/demo/src/lib.rs", "sized"),
                ("crates/demo/src/lib.rs", "raw"),
            },
        )
        # Definitions of start wrappers are not starts.
        self.assertEqual(
            set(starts),
            {
                ("crates/demo/src/lib.rs", name)
                for name in ("launch", "sized", "raw", "native", "detached")
            },
        )
        self.assertEqual(MODULE.main(["--root", str(root)]), 1)

    def test_the_repository_roster_matches_the_current_tree(self):
        self.assertEqual(MODULE.main([]), 0)


if __name__ == "__main__":
    unittest.main()
