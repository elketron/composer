use crate::provider::Usage;

/// The tokens one turn has used (`agent.turn_tokens`): what the model wrote plus what each step
/// brought into the context. The context the turn started with isn't counted, nor the prompt
/// sent again on every step. Without usage from the provider, text is estimated at 4 chars a token.
pub struct TurnBudget {
    limit: u64,
    used: u64,
    /// The prompt the next call would have if nothing were added: the last prompt plus what was
    /// written. Its growth beyond this is what the tools brought in.
    next_prompt: Option<u64>,
    /// The last call had no usage: its tools' results are estimated.
    estimating: bool,
}

impl TurnBudget {
    /// `limit` 0: no limit.
    pub fn new(limit: u32) -> Self {
        Self {
            limit: limit as u64,
            used: 0,
            next_prompt: None,
            estimating: false,
        }
    }

    /// After a model call, with its usage if the provider gave one, and what it wrote.
    pub fn model_call(&mut self, usage: Option<&Usage>, written: &str) {
        match usage {
            Some(usage) => {
                let (prompt, completion) =
                    (usage.prompt_tokens as u64, usage.completion_tokens as u64);
                if let Some(expected) = self.next_prompt {
                    self.used += prompt.saturating_sub(expected);
                }
                self.used += completion;
                self.next_prompt = Some(prompt + completion);
                self.estimating = false;
            }
            None => {
                self.used += estimate(written);
                self.next_prompt = None;
                self.estimating = true;
            }
        }
    }

    /// After the step's tools ran. With usage, the next call's prompt counts them instead.
    pub fn tool_results(&mut self, results: &str) {
        if self.estimating {
            self.used += estimate(results);
        }
    }

    pub fn exceeded(&self) -> bool {
        self.limit > 0 && self.used >= self.limit
    }

    pub fn used(&self) -> u64 {
        self.used
    }
}

fn estimate(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(prompt: u32, completion: u32) -> Usage {
        Usage {
            prompt_tokens: prompt,
            completion_tokens: completion,
        }
    }

    #[test]
    fn counts_what_is_written_and_brought_in_not_the_context() {
        let mut budget = TurnBudget::new(1_000);
        // A turn in a session that is already 100k tokens long.
        budget.model_call(Some(&usage(100_000, 50)), "");
        assert_eq!(budget.used(), 50);
        // The tool result added 300 tokens to the prompt.
        budget.tool_results("ignored: the next prompt says");
        budget.model_call(Some(&usage(100_350, 40)), "");
        assert_eq!(budget.used(), 50 + 300 + 40);
        assert!(!budget.exceeded());
        budget.model_call(Some(&usage(101_000, 20)), "");
        assert_eq!(budget.used(), 390 + 610 + 20);
        assert!(budget.exceeded());
    }

    #[test]
    fn compaction_doesnt_count_as_negative() {
        let mut budget = TurnBudget::new(0);
        budget.model_call(Some(&usage(150_000, 100)), "");
        budget.model_call(Some(&usage(20_000, 100)), "");
        assert_eq!(budget.used(), 200);
        assert!(!budget.exceeded(), "0 is no limit");
    }

    #[test]
    fn estimates_without_usage() {
        let mut budget = TurnBudget::new(10);
        budget.model_call(None, "abcdefgh");
        budget.tool_results("abcdefghijkl");
        assert_eq!(budget.used(), 2 + 3);
        assert!(!budget.exceeded());
        budget.tool_results(&"x".repeat(20));
        assert!(budget.exceeded());
    }
}
