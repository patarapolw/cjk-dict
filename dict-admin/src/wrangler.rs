//! Thin wrapper over the `wrangler` CLI for D1 and R2.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Wrangler {
    /// e.g. `node worker/node_modules/wrangler/bin/wrangler.js`
    pub cmd: Vec<String>,
    /// directory holding wrangler.toml
    pub worker_dir: PathBuf,
    /// D1 database name or binding
    pub database: String,
    /// R2 bucket name
    pub bucket: String,
    /// use local (miniflare) storage instead of Cloudflare
    pub local: bool,
}

impl Wrangler {
    fn command(&self) -> Command {
        let mut c = Command::new(&self.cmd[0]);
        c.args(&self.cmd[1..]).current_dir(&self.worker_dir);
        c
    }

    fn target(&self) -> &'static str {
        if self.local { "--local" } else { "--remote" }
    }

    fn run(&self, args: &[&str], capture: bool) -> Result<String> {
        let mut c = self.command();
        c.args(args);
        c.stdin(Stdio::null());
        if capture {
            c.stdout(Stdio::piped());
        }
        let out = c
            .spawn()
            .with_context(|| format!("failed to start {:?}", self.cmd))?
            .wait_with_output()?;
        if !out.status.success() {
            bail!(
                "wrangler {} failed ({}): {}",
                args.first().copied().unwrap_or(""),
                out.status,
                String::from_utf8_lossy(&out.stdout)
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Run SQL and return the `results` of every statement.
    pub fn query(&self, sql: &str) -> Result<Vec<Vec<Value>>> {
        let stdout = self.run(
            &[
                "d1",
                "execute",
                &self.database,
                self.target(),
                "--json",
                "--yes",
                "--command",
                sql,
            ],
            true,
        )?;
        parse_results(&stdout).with_context(|| format!("parsing wrangler output for: {sql}"))
    }

    /// Rows of the first statement.
    pub fn query_rows(&self, sql: &str) -> Result<Vec<Value>> {
        Ok(self.query(sql)?.into_iter().next().unwrap_or_default())
    }

    pub fn execute_file(&self, path: &Path) -> Result<()> {
        let path = std::path::absolute(path)?;
        self.run(
            &[
                "d1",
                "execute",
                &self.database,
                self.target(),
                "--yes",
                "--file",
                &path.to_string_lossy(),
            ],
            true,
        )?;
        Ok(())
    }

    pub fn migrate(&self) -> Result<()> {
        self.run(
            &["d1", "migrations", "apply", &self.database, self.target()],
            false,
        )?;
        Ok(())
    }

    pub fn r2_put(&self, key: &str, file: &Path, content_type: &str) -> Result<()> {
        let file = std::path::absolute(file)?;
        self.run(
            &[
                "r2",
                "object",
                "put",
                &format!("{}/{}", self.bucket, key),
                self.target(),
                "--file",
                &file.to_string_lossy(),
                "--content-type",
                content_type,
            ],
            false,
        )?;
        Ok(())
    }

    pub fn r2_delete(&self, key: &str) -> Result<()> {
        self.run(
            &[
                "r2",
                "object",
                "delete",
                &format!("{}/{}", self.bucket, key),
                self.target(),
            ],
            false,
        )?;
        Ok(())
    }
}

fn parse_results(stdout: &str) -> Result<Vec<Vec<Value>>> {
    // wrangler may print banners (e.g. "[WARNING] ...") before the JSON:
    // take the first '[' from which a JSON array parses
    let v: Value = stdout
        .match_indices('[')
        .find_map(|(i, _)| {
            let mut de = serde_json::Deserializer::from_str(&stdout[i..]).into_iter::<Value>();
            match de.next() {
                Some(Ok(v)) if v.is_array() => Some(v),
                _ => None,
            }
        })
        .context("no JSON array in wrangler output")?;
    let arr = v.as_array().context("expected JSON array")?;
    Ok(arr
        .iter()
        .map(|r| {
            r.get("results")
                .and_then(|x| x.as_array())
                .cloned()
                .unwrap_or_default()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse() {
        let out =
            "\u{1b}[33m[WARNING] proxy\n[{\"results\":[{\"id\":1}],\"success\":true,\"meta\":{}}]";
        let r = super::parse_results(out).unwrap();
        assert_eq!(r[0][0]["id"], 1);
    }
}
