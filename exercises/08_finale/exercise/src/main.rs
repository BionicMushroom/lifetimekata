use require_lifetimes::require_lifetimes;
use std::collections::LinkedList;

#[derive(Debug, PartialEq, Eq)]
enum MatcherToken<'a> {
    /// This is just text without anything special.
    RawText(&'a str),
    /// This is when text could be any one of multiple
    /// strings. It looks like `(one|two|three)`, where
    /// `one`, `two` or `three` are the allowed strings.
    OneOfText(Vec<&'a str>),
    /// This is when you're happy to accept any single character.
    /// It looks like `.`
    WildCard,
}

struct OptionalInputData<'reference, 'matcher_token, 'str_to_match> {
    chosen_option: (&'reference MatcherToken<'matcher_token>, &'str_to_match str),
    parent_frame_index: usize,
}

struct InputData<'reference, 'matcher_token, 'str_to_match> {
    tokens: &'reference [MatcherToken<'matcher_token>],
    string: &'str_to_match str,
    optional_data: Option<OptionalInputData<'reference, 'matcher_token, 'str_to_match>>,
}

struct OptionalOutputData {
    parent_frame_index: usize,
}

struct OutputData<'reference, 'matcher_token, 'str_to_match> {
    matched_tokens: LinkedList<Vec<(&'reference MatcherToken<'matcher_token>, &'str_to_match str)>>,
    matched_tokens_count: usize,
    best_current_matched_tokens:
        LinkedList<Vec<(&'reference MatcherToken<'matcher_token>, &'str_to_match str)>>,
    best_current_matched_tokens_count: usize,
    is_complete_match: bool,
    optional_data: Option<OptionalOutputData>,
}

enum Frame<'reference, 'matcher_token, 'str_to_match> {
    Input(InputData<'reference, 'matcher_token, 'str_to_match>),
    Output(OutputData<'reference, 'matcher_token, 'str_to_match>),
}

#[derive(Debug, PartialEq, Eq)]
struct Matcher<'a> {
    /// This is the actual text of the matcher
    text: &'a str,
    /// This is a vector of the tokens inside the expression.
    tokens: Vec<MatcherToken<'a>>,
    /// This keeps track of the most tokens that this matcher has matched.
    most_tokens_matched: usize,
}

impl<'internal> Matcher<'internal> {
    /// This should take a string reference, and return
    /// an `Matcher` which has parsed that reference.
    #[require_lifetimes]
    fn new(text: &'internal str) -> Option<Matcher<'internal>> {
        let mut unparsed_text = text;
        let mut tokens = Vec::new();

        while let Some(dot_paren_index) = unparsed_text.find(['.', '(']) {
            let raw_text = &unparsed_text[..dot_paren_index];
            if !raw_text.is_empty() {
                tokens.push(MatcherToken::RawText(raw_text));
            }

            if unparsed_text.as_bytes()[dot_paren_index] == b'.' {
                tokens.push(MatcherToken::WildCard);
                unparsed_text = &unparsed_text[dot_paren_index + 1..];
            } else {
                unparsed_text = &unparsed_text[dot_paren_index + 1..];
                let mut options = Vec::new();
                let mut found_a_pipe = false;

                loop {
                    if let Some(pipe_paren_index) = unparsed_text.find(['|', ')']) {
                        if unparsed_text.as_bytes()[pipe_paren_index] == b'|' {
                            let option = &unparsed_text[..pipe_paren_index];
                            if option.is_empty() {
                                return None;
                            }

                            options.push(option);
                            found_a_pipe = true;
                            unparsed_text = &unparsed_text[pipe_paren_index + 1..];
                        } else {
                            let option: &str = &unparsed_text[..pipe_paren_index];
                            if option.is_empty() || !found_a_pipe {
                                return None;
                            }

                            options.push(option);
                            unparsed_text = &unparsed_text[pipe_paren_index + 1..];
                            break;
                        }
                    } else {
                        return None;
                    }
                }

                tokens.push(MatcherToken::OneOfText(options));
            }
        }

        if !unparsed_text.is_empty() || tokens.is_empty() {
            tokens.push(MatcherToken::RawText(unparsed_text));
        }

        Some(Matcher {
            text,
            tokens,
            most_tokens_matched: 0,
        })
    }

    /// This should take a string, and return a vector of tokens, and the corresponding part
    /// of the given string. For examples, see the test cases below.
    #[require_lifetimes]
    fn match_string<'a, 'b>(
        &'a mut self,
        string: &'b str,
    ) -> Vec<(&'a MatcherToken<'internal>, &'b str)> {
        let mut matched_tokens = Vec::new();
        let mut string = string;

        for token in &self.tokens {
            match token {
                MatcherToken::RawText(text) => {
                    if !Self::match_raw_text(text, token, &mut matched_tokens, &mut string) {
                        break;
                    }
                }
                MatcherToken::OneOfText(options) => {
                    if !Self::match_one_of_text(options, token, &mut matched_tokens, &mut string) {
                        break;
                    }
                }
                MatcherToken::WildCard => {
                    if !Self::match_wild_card(token, &mut matched_tokens, &mut string) {
                        break;
                    }
                }
            }
        }

        if matched_tokens.len() > self.most_tokens_matched {
            self.most_tokens_matched = matched_tokens.len();
        }

        matched_tokens
    }

    /// This should try all possible combinations while attempting to find a match.
    /// Even if the code is uglier, I chose to use a heap-allocated stack
    /// rather than going with a recursive implementation so as to not be
    /// limited by the thread stack.
    #[require_lifetimes]
    fn match_string_exhaustive<'a, 'b>(
        &'a mut self,
        string: &'b str,
    ) -> Vec<(&'a MatcherToken<'internal>, &'b str)> {
        let mut stack = vec![Frame::Input(InputData {
            tokens: &self.tokens,
            string,
            optional_data: None,
        })];

        while let Some(frame) = stack.pop() {
            match frame {
                Frame::Input(input_data) => {
                    Self::process_input_frame(input_data, &mut stack);
                }
                Frame::Output(output_data) => {
                    if let Some(matched_tokens) =
                        Self::process_output_frame(output_data, &mut stack)
                    {
                        if matched_tokens.len() > self.most_tokens_matched {
                            self.most_tokens_matched = matched_tokens.len();
                        }

                        return matched_tokens;
                    }
                }
            }
        }

        unreachable!();
    }

    #[require_lifetimes]
    fn match_raw_text<'a, 'b, 'c, 'd, 'e, 'f>(
        text: &'a str,
        token: &'b MatcherToken<'c>,
        matched_tokens: &'d mut Vec<(&'b MatcherToken<'c>, &'e str)>,
        string: &'f mut &'e str,
    ) -> bool {
        if string.starts_with(text) {
            matched_tokens.push((token, &string[..text.len()]));
            *string = &string[text.len()..];
            true
        } else {
            false
        }
    }

    #[require_lifetimes]
    fn match_one_of_text<'a, 'b, 'c, 'd, 'e, 'f>(
        options: &'a Vec<&'b str>,
        token: &'c MatcherToken<'b>,
        matched_tokens: &'d mut Vec<(&'c MatcherToken<'b>, &'e str)>,
        string: &'f mut &'e str,
    ) -> bool {
        if let Some(option) = options.iter().find(|&option| string.starts_with(option)) {
            matched_tokens.push((token, &string[..option.len()]));
            *string = &string[option.len()..];
            true
        } else {
            false
        }
    }

    #[require_lifetimes]
    fn match_one_of_text_exhaustive<'a, 'b, 'c>(
        options: &'a Vec<&'b str>,
        token: &'c MatcherToken<'b>,
        index: usize,
        string: &'a str,
    ) -> impl Iterator<Item = (usize, &'c MatcherToken<'b>, &'b str)> + 'a
    where
        'c: 'a,
    {
        options
            .iter()
            .filter(|&option| string.starts_with(option))
            .map(move |&option| (index, token, option))
    }

    #[require_lifetimes]
    fn match_wild_card<'a, 'b, 'c, 'd, 'e>(
        token: &'a MatcherToken<'b>,
        matched_tokens: &'c mut Vec<(&'a MatcherToken<'b>, &'d str)>,
        string: &'e mut &'d str,
    ) -> bool {
        if let Some(c) = string.chars().next() {
            let next_char_index = c.len_utf8();
            matched_tokens.push((token, &string[..next_char_index]));
            *string = &string[next_char_index..];
            true
        } else {
            false
        }
    }

    #[require_lifetimes]
    fn process_input_frame<'a, 'b, 'c, 'd>(
        mut input_data: InputData<'a, 'b, 'c>,
        stack: &'d mut Vec<Frame<'a, 'b, 'c>>,
    ) {
        let mut matched_tokens = input_data
            .optional_data
            .as_ref()
            .map_or_else(|| Vec::new(), |d| vec![d.chosen_option]);
        let mut options_iter = None;

        for (index, token) in input_data.tokens.iter().enumerate() {
            match token {
                MatcherToken::RawText(text) => {
                    if !Self::match_raw_text(
                        text,
                        token,
                        &mut matched_tokens,
                        &mut input_data.string,
                    ) {
                        break;
                    }
                }
                MatcherToken::OneOfText(options) => {
                    options_iter = Some(Self::match_one_of_text_exhaustive(
                        &options,
                        token,
                        index,
                        &input_data.string,
                    ));
                    break;
                }
                MatcherToken::WildCard => {
                    if !Self::match_wild_card(&token, &mut matched_tokens, &mut input_data.string) {
                        break;
                    }
                }
            }
        }

        let matched_tokens_count = matched_tokens.len();
        let mut matched_tokens_list = LinkedList::new();

        if !matched_tokens.is_empty() {
            matched_tokens_list.push_back(matched_tokens);
        }

        let output_frame_index = stack.len();

        stack.push(Frame::Output(OutputData {
            matched_tokens: matched_tokens_list,
            matched_tokens_count,
            best_current_matched_tokens: LinkedList::new(),
            best_current_matched_tokens_count: 0,
            is_complete_match: input_data.tokens.len() == matched_tokens_count,
            optional_data: input_data
                .optional_data
                .as_ref()
                .map(|d| OptionalOutputData {
                    parent_frame_index: d.parent_frame_index,
                }),
        }));

        for (index, token, option) in options_iter.into_iter().flatten() {
            stack.push(Frame::Input(InputData {
                tokens: &input_data.tokens[index + 1..],
                string: &input_data.string[option.len()..],
                optional_data: Some(OptionalInputData {
                    chosen_option: (token, &input_data.string[..option.len()]),
                    parent_frame_index: output_frame_index,
                }),
            }));
        }
    }

    #[require_lifetimes]
    fn process_output_frame<'a, 'b, 'c, 'd>(
        mut output_data: OutputData<'a, 'b, 'c>,
        stack: &'d mut Vec<Frame<'a, 'b, 'c>>,
    ) -> Option<Vec<(&'a MatcherToken<'b>, &'c str)>> {
        output_data.matched_tokens_count += output_data.best_current_matched_tokens_count;
        output_data
            .matched_tokens
            .append(&mut output_data.best_current_matched_tokens);

        if let Some(optional_data) = output_data.optional_data {
            if let Frame::Output(parent_output_data) = &mut stack[optional_data.parent_frame_index]
            {
                if output_data.is_complete_match {
                    if !parent_output_data.is_complete_match
                        || output_data.matched_tokens_count
                            > parent_output_data.best_current_matched_tokens_count
                    {
                        parent_output_data.best_current_matched_tokens_count =
                            output_data.matched_tokens_count;
                        parent_output_data.best_current_matched_tokens = output_data.matched_tokens;
                        parent_output_data.is_complete_match = true;
                    }
                } else {
                    if !parent_output_data.is_complete_match
                        && output_data.matched_tokens_count
                            > parent_output_data.best_current_matched_tokens_count
                    {
                        parent_output_data.best_current_matched_tokens_count =
                            output_data.matched_tokens_count;
                        parent_output_data.best_current_matched_tokens = output_data.matched_tokens;
                    }
                }

                None
            } else {
                unreachable!();
            }
        } else {
            Some(output_data.matched_tokens.into_iter().flatten().collect())
        }
    }
}

fn main() {
    unimplemented!()
}

#[cfg(test)]
mod test {
    use super::{Matcher, MatcherToken};
    #[test]
    fn simple_test() {
        let match_string = "abc(d|e|f).".to_string();
        let mut matcher = Matcher::new(&match_string).unwrap();

        assert_eq!(matcher.most_tokens_matched, 0);

        {
            let candidate1 = "abcge".to_string();
            let result = matcher.match_string(&candidate1);
            assert_eq!(result, vec![(&MatcherToken::RawText("abc"), "abc"),]);
            assert_eq!(matcher.most_tokens_matched, 1);
        }

        {
            let candidate1 = "abcde".to_string();
            let result = matcher.match_string(&candidate1);
            assert_eq!(
                result,
                vec![
                    (&MatcherToken::RawText("abc"), "abc"),
                    (&MatcherToken::OneOfText(vec!["d", "e", "f"]), "d"),
                    (&MatcherToken::WildCard, "e")
                ]
            );
            assert_eq!(matcher.most_tokens_matched, 3);
        }

        {
            let candidate1 = "abcd💪".to_string();
            let result = matcher.match_string(&candidate1);
            assert_eq!(
                result,
                vec![
                    (&MatcherToken::RawText("abc"), "abc"),
                    (&MatcherToken::OneOfText(vec!["d", "e", "f"]), "d"),
                    (&MatcherToken::WildCard, "💪")
                ]
            );
            assert_eq!(matcher.most_tokens_matched, 3);
        }
    }

    #[test]
    fn simple_test_with_exhaustive_match() {
        let match_string = "abc(d|e|f).".to_string();
        let mut matcher = Matcher::new(&match_string).unwrap();

        assert_eq!(matcher.most_tokens_matched, 0);

        {
            let candidate1 = "abcge".to_string();
            let result = matcher.match_string_exhaustive(&candidate1);
            assert_eq!(result, vec![(&MatcherToken::RawText("abc"), "abc"),]);
            assert_eq!(matcher.most_tokens_matched, 1);
        }

        {
            let candidate1 = "abcde".to_string();
            let result = matcher.match_string_exhaustive(&candidate1);
            assert_eq!(
                result,
                vec![
                    (&MatcherToken::RawText("abc"), "abc"),
                    (&MatcherToken::OneOfText(vec!["d", "e", "f"]), "d"),
                    (&MatcherToken::WildCard, "e")
                ]
            );
            assert_eq!(matcher.most_tokens_matched, 3);
        }

        {
            let candidate1 = "abcd💪".to_string();
            let result = matcher.match_string_exhaustive(&candidate1);
            assert_eq!(
                result,
                vec![
                    (&MatcherToken::RawText("abc"), "abc"),
                    (&MatcherToken::OneOfText(vec!["d", "e", "f"]), "d"),
                    (&MatcherToken::WildCard, "💪")
                ]
            );
            assert_eq!(matcher.most_tokens_matched, 3);
        }
    }

    #[test]
    fn exhaustive_match() {
        let match_string = "(aba|abac).(aba|abac).";
        let mut matcher = Matcher::new(&match_string).unwrap();

        assert_eq!(matcher.most_tokens_matched, 0);

        let candidate = "abacabacd";
        let result = matcher.match_string(candidate);
        assert_eq!(
            result,
            vec![
                (&MatcherToken::OneOfText(vec!["aba", "abac"]), "aba"),
                (&MatcherToken::WildCard, "c"),
                (&MatcherToken::OneOfText(vec!["aba", "abac"]), "aba"),
                (&MatcherToken::WildCard, "c")
            ]
        );
        assert_eq!(matcher.most_tokens_matched, 4);
    }

    #[test]
    fn exhaustive_match_with_exhaustive_matcher() {
        let match_string = "(aba|abac).(aba|abac).";
        let mut matcher = Matcher::new(&match_string).unwrap();

        assert_eq!(matcher.most_tokens_matched, 0);

        let candidate = "abacabacd";
        let result = matcher.match_string_exhaustive(candidate);
        assert_eq!(
            result,
            vec![
                (&MatcherToken::OneOfText(vec!["aba", "abac"]), "aba"),
                (&MatcherToken::WildCard, "c"),
                (&MatcherToken::OneOfText(vec!["aba", "abac"]), "abac"),
                (&MatcherToken::WildCard, "d")
            ]
        );
        assert_eq!(matcher.most_tokens_matched, 4);
    }

    #[test]
    fn broken_matcher() {
        let match_string = "abc(d|e|f.".to_string();
        let matcher = Matcher::new(&match_string);
        assert_eq!(matcher, None);
    }
}
