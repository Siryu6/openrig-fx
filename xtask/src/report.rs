//! Check results: printed as they come, summed up at the end

use std::process::ExitCode;

pub struct Report {
    strict: bool,
    failures: usize,
    skips: usize,
}

impl Report {
    pub fn new(strict: bool) -> Self {
        Self { strict, failures: 0, skips: 0 }
    }

    pub fn pass(&mut self, check: &str, what: &str) {
        println!("  ok    {check:<10} {what}");
    }

    pub fn fail(&mut self, check: &str, why: &str) {
        self.failures += 1;
        println!("  FAIL  {check:<10} {why}");
    }

    /// Something worth knowing that is not a failure
    pub fn note(&mut self, check: &str, what: &str) {
        println!("  note  {check:<10} {what}");
    }

    /// A check that cannot run here (missing tool, foreign architecture)
    pub fn skip(&mut self, check: &str, why: &str) {
        self.skips += 1;
        println!("  skip  {check:<10} {why}");
    }

    pub fn failed(&self) -> bool {
        self.failures > 0
    }

    pub fn finish(self) -> ExitCode {
        let skips_fail = self.strict && self.skips > 0;
        println!(
            "{} failed, {} skipped{}",
            self.failures,
            self.skips,
            if skips_fail { " (--strict: skips fail)" } else { "" }
        );
        if self.failures > 0 || skips_fail { ExitCode::FAILURE } else { ExitCode::SUCCESS }
    }
}
