/*
 * Copyright 2026 Julien Bombled
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Whether a keyboard-interactive round asks for the account's password and nothing else,
//! as the C# `LooksLikePasswordPrompt` and `LooksLikeOneTimeCodePrompt` tell
//! (`SshConnectionFactory.cs`), stricter: one prompt, typed hidden, made only of words a
//! plain password prompt is made of, with a title and instructions as plain or empty.
//!
//! A round wrongly taken for a password sends the secret to the wrong factor, which cannot
//! be undone; a password round missed only asks the user. So the round is matched against
//! what it may say, never against what it may not: a word outside the list ("Temporary",
//! "RADIUS", "admin", "Einmalpasswort") is a doubt, and every doubt is a "no".
//!
//! The title and instructions are held to the same list, a few plain words added: a PAM
//! notice ("Your password will expire in 5 days") then sends the round to the user, who
//! reads it in the keyboard-interactive dialog instead of losing it behind the password.

use crate::prompter::KeyboardInteractiveQuestion;

/// Words that name the account's password (C# `PasswordPromptMarkers`, "passwd" and
/// "kennwort" added). "Passphrase" is left out: it names a key's, not an account's.
const PASSWORD_WORDS: &[&str] = &[
    "password",
    "passwd",
    "passwort",
    "kennwort",
    "contraseña",
    "contrasena",
];

/// Consecutive words that name the account's password.
const PASSWORD_PHRASES: &[&[&str]] = &[&["mot", "de", "passe"]];

/// The other words a plain password prompt is made of, in the languages above: "Enter your
/// LDAP password", "Mot de passe de connexion", "Bitte geben Sie Ihr Passwort ein",
/// "Ingrese su contraseña".
const PLAIN_WORDS: &[&str] = &[
    // English.
    "enter",
    "your",
    "the",
    "please",
    "ldap",
    "unix",
    "login",
    "logon",
    "account",
    // French.
    "mot",
    "de",
    "passe",
    "entrez",
    "saisissez",
    "veuillez",
    "saisir",
    "votre",
    "le",
    "du",
    "compte",
    "connexion",
    // German.
    "bitte",
    "geben",
    "sie",
    "ihr",
    "ein",
    "eingeben",
    // Spanish.
    "ingrese",
    "introduzca",
    "escriba",
    "su",
    "por",
    "favor",
    "la",
    "cuenta",
];

/// Words a plain title or instructions may say besides a prompt's: "Password
/// authentication", "keyboard-interactive", "Please enter your password to continue".
const PLAIN_NOTICE_WORDS: &[&str] = &[
    "to",
    "a",
    "an",
    "and",
    "now",
    "continue",
    "required",
    "authentication",
    "authentification",
    "keyboard",
    "interactive",
    "pam",
];

/// Words after which a prompt names the account it asks about ("Password for jdoe:"):
/// allowed only before the user's own account.
const ACCOUNT_INTRODUCERS: &[&str] = &["for", "pour", "für", "fur", "fuer", "para"];

/// Words after which the user's own account stands as an account, besides the introducers:
/// the French and Spanish "de" ("Mot de passe de jdoe", "Contraseña de jdoe").
const ACCOUNT_OF: &[&str] = &["de"];

/// The possessive of "jdoe@host's password": allowed only after the user's own account.
const POSSESSIVE: &str = "s";

/// Whole words that name a one-time code or a second factor (C# `OneTimeCodePromptPattern`,
/// widened), checked in the title, the instructions and a host label.
const ONE_TIME_CODE_WORDS: &[&str] = &[
    "otp", "totp", "hotp", "oath", "pin", "duo", "sms", "tfa", "verify",
];

/// Pieces of words that name a one-time code or a second factor, compounds included
/// ("tokencode", "OneTimePassword", "Zweifaktorauthentifizierung"), matched anywhere in a
/// word of the title, the instructions or a host label.
const ONE_TIME_CODE_STEMS: &[&str] = &[
    "otp",
    "token",
    "code",
    "codigo",
    "código",
    "onetime",
    "securid",
    "passcode",
    "yubi",
    "radius",
    "second",
    "tempor",
    "dynami",
    "faktor",
    "factor",
    "facteur",
    "mfa",
    "2fa",
    "a2f",
    "einmal",
    "jeton",
    "verif",
    "vérif",
    "unique",
    "authenticator",
    "authentificateur",
];

/// Consecutive words that name a one-time code or a second factor.
const ONE_TIME_CODE_PHRASES: &[&[&str]] = &[
    &["one", "time"],
    &["two", "step"],
    &["2", "step"],
    &["deux", "étapes"],
    &["un", "solo", "uso"],
];

/// What separates a host from the account in front of it ("jdoe@host").
const AT: char = '@';

/// What opens and closes an IPv6 address as a host label ("jdoe@[fe80::1]").
const ADDRESS_OPEN: char = '[';
const ADDRESS_CLOSE: char = ']';

/// A piece of a prompt: a word, or the user's own account, `at_host` when a host follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Word(&'a str),
    Own { at_host: bool },
}

/// Characters an account name is made of, domain and machine accounts included
/// (`CORP\jdoe`, `host$`): the user's own name inside a longer one is not the user's.
fn is_account_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '\\' | '$' | AT)
}

/// Characters of a host label after "jdoe@" outside brackets: a `:` ends it, so
/// "jdoe@host:root's" leaves "root" to be read.
fn is_host_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// `true` when a keyboard-interactive round is to be asked as the password method's
/// question: it asks for the account's password alone, and no password question was asked
/// on this connection yet (`password_questions`). Asked again (a refusal, or a second factor
/// worded as a password), the round is the user's: a secret is never sent twice in a loop,
/// and a password already given never goes to a later factor (C# `MarkPasswordSpent`).
pub(crate) fn round_is_the_password(
    password_questions: u32,
    question: &KeyboardInteractiveQuestion,
) -> bool {
    password_questions == 0 && asks_for_the_account_password(question)
}

/// `true` when `question` asks for the account's password alone: exactly one prompt, typed
/// hidden, made only of a plain password prompt's words and the user's own account, in a
/// round whose title and instructions are empty or as plain, and name no one-time code.
pub(crate) fn asks_for_the_account_password(question: &KeyboardInteractiveQuestion) -> bool {
    let [prompt] = question.prompts.as_slice() else {
        return false;
    };
    if prompt.echo {
        return false;
    }
    let username = question.username.to_lowercase();
    let text = prompt.text.to_lowercase();
    let Some(tokens) = tokens(&text, &username) else {
        return false;
    };
    if !names_a_password(&tokens) || !is_plain(&tokens, &[]) {
        return false;
    }
    [&question.name, &question.instructions]
        .into_iter()
        .all(|said| is_a_plain_notice(&said.to_lowercase(), &username))
}

/// `true` when lowercased `said` (a title or instructions) is empty, or names no one-time
/// code and is made only of a plain prompt's words, plain notice words and the user's own
/// account.
fn is_a_plain_notice(said: &str, username: &str) -> bool {
    if names_a_one_time_code(said) {
        return false;
    }
    tokens(said, username).is_some_and(|tokens| is_plain(&tokens, PLAIN_NOTICE_WORDS))
}

/// The words of `tokens`, the user's own account left out.
fn words_of<'a>(tokens: &[Token<'a>]) -> Vec<&'a str> {
    tokens
        .iter()
        .filter_map(|token| match token {
            Token::Word(word) => Some(*word),
            Token::Own { .. } => None,
        })
        .collect()
}

fn names_a_password(tokens: &[Token<'_>]) -> bool {
    let words = words_of(tokens);
    words.iter().any(|word| PASSWORD_WORDS.contains(word))
        || PASSWORD_PHRASES
            .iter()
            .any(|phrase| words.windows(phrase.len()).any(|window| window == *phrase))
}

/// `true` when every token is allowed where it stands: a word of the lists (`more` added),
/// an introducer before the user's own account, a possessive after it, and the user's own
/// account only where it stands as an account (with its host, after an introducer or "de",
/// or before a possessive). A bare account name is a doubt: user "token" and "Token
/// password:" name a token.
fn is_plain(tokens: &[Token<'_>], more: &[&str]) -> bool {
    tokens.iter().enumerate().all(|(index, token)| {
        let before = index.checked_sub(1).map(|at| tokens[at]);
        let after = tokens.get(index + 1).copied();
        match token {
            Token::Own { at_host } => {
                *at_host
                    || matches!(before, Some(Token::Word(word))
                        if ACCOUNT_INTRODUCERS.contains(&word) || ACCOUNT_OF.contains(&word))
                    || after == Some(Token::Word(POSSESSIVE))
            }
            Token::Word(word) if ACCOUNT_INTRODUCERS.contains(word) => {
                matches!(after, Some(Token::Own { .. }))
            }
            Token::Word(word) if *word == POSSESSIVE => matches!(before, Some(Token::Own { .. })),
            Token::Word(word) => {
                PASSWORD_WORDS.contains(word) || PLAIN_WORDS.contains(word) || more.contains(word)
            }
        }
    })
}

/// The tokens of lowercased `text`, the whole-name occurrences of lowercased `username`
/// (with the `@host` after one) taken as [`Token::Own`]. `None` when an `@` is left
/// (another account: "(admin@host) Password:"), or a host label is a doubt.
fn tokens<'a>(text: &'a str, username: &str) -> Option<Vec<Token<'a>>> {
    let mut tokens = Vec::new();
    let mut copied = 0;
    if !username.is_empty() {
        for (start, found) in text.match_indices(username) {
            if start < copied {
                continue;
            }
            let mut end = start + found.len();
            let before = text[..start].chars().next_back();
            let after = text[end..].chars().next();
            let whole_name = !before.is_some_and(is_account_char)
                && !after.is_some_and(|c| c != AT && is_account_char(c));
            if !whole_name {
                continue;
            }
            let at_host = after == Some(AT);
            if at_host {
                end += AT.len_utf8();
                end += host_label_length(&text[end..])?;
            }
            plain_tokens(&text[copied..start], &mut tokens)?;
            tokens.push(Token::Own { at_host });
            copied = end;
        }
    }
    plain_tokens(&text[copied..], &mut tokens)?;
    Some(tokens)
}

/// The length of the host label at the start of `rest`, after "jdoe@": a bracketed address,
/// or host characters up to the first other one. `None` for an unclosed bracket, or a label
/// naming a one-time code ("(jdoe@otp) Password:").
fn host_label_length(rest: &str) -> Option<usize> {
    let length = if rest.starts_with(ADDRESS_OPEN) {
        rest.find(ADDRESS_CLOSE)? + ADDRESS_CLOSE.len_utf8()
    } else {
        rest.chars()
            .take_while(|&c| is_host_char(c))
            .map(char::len_utf8)
            .sum()
    };
    (!names_a_one_time_code(&rest[..length])).then_some(length)
}

/// Appends the words of `piece`, which holds no own account; `None` when it holds an `@`.
fn plain_tokens<'a>(piece: &'a str, tokens: &mut Vec<Token<'a>>) -> Option<()> {
    if piece.contains(AT) {
        return None;
    }
    tokens.extend(words(piece).into_iter().map(Token::Word));
    Some(())
}

/// The words of lowercased `text`, split on anything but letters and digits.
fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect()
}

/// `true` when lowercased `text` names a one-time code or a second factor: a word of the
/// list, a piece of a word, or consecutive words.
fn names_a_one_time_code(text: &str) -> bool {
    let words = words(text);
    words.iter().any(|word| {
        ONE_TIME_CODE_WORDS.contains(word)
            || ONE_TIME_CODE_STEMS.iter().any(|stem| word.contains(stem))
    }) || ONE_TIME_CODE_PHRASES
        .iter()
        .any(|phrase| words.windows(phrase.len()).any(|window| window == *phrase))
}

#[cfg(test)]
mod tests {
    use super::{asks_for_the_account_password, round_is_the_password};
    use crate::prompter::{KeyboardInteractivePrompt, KeyboardInteractiveQuestion};

    const USER: &str = "jdoe";

    fn round(
        user: &str,
        instructions: &str,
        prompts: &[(&str, bool)],
    ) -> KeyboardInteractiveQuestion {
        KeyboardInteractiveQuestion {
            host: "host".to_owned(),
            username: user.to_owned(),
            name: String::new(),
            instructions: instructions.to_owned(),
            prompts: prompts
                .iter()
                .map(|(text, echo)| KeyboardInteractivePrompt {
                    text: (*text).to_owned(),
                    echo: *echo,
                })
                .collect(),
        }
    }

    fn password_round(user: &str, prompt: &str) -> bool {
        asks_for_the_account_password(&round(user, "", &[(prompt, false)]))
    }

    #[test]
    fn a_password_prompt_is_the_password_in_every_language() {
        for prompt in [
            "Password: ",
            "password:",
            "PASSWORD:",
            "Password",
            "Enter password:",
            "Enter your password:",
            "Please enter your password: ",
            "LDAP Password: ",
            "Login password: ",
            "Account password: ",
            "Mot de passe : ",
            "Mot de passe de connexion :",
            "Entrez votre mot de passe :",
            "Passwort: ",
            "Bitte geben Sie Ihr Passwort ein:",
            "Kennwort:",
            "Contraseña: ",
            "Contrasena: ",
            "Ingrese su contraseña: ",
        ] {
            assert!(password_round(USER, prompt), "{prompt:?}");
        }
    }

    #[test]
    fn a_prompt_naming_the_user_s_own_account_is_the_password() {
        for prompt in [
            "(jdoe@host) Password: ",
            "(JDOE@10.0.0.1) Password: ",
            "(jdoe@[fe80::1%eth0]) Password: ",
            "jdoe@host's password: ",
            "jdoe@srv-01.corp.local's password: ",
            "Password for jdoe:",
            "Password for jdoe@CORP.LOCAL: ",
            "Password for `jdoe':",
            "Mot de passe pour jdoe :",
            "jdoe's password: ",
            "Mot de passe de jdoe :",
            "Passwort für jdoe:",
            "Contraseña para jdoe:",
            "Contraseña de jdoe:",
        ] {
            assert!(password_round(USER, prompt), "{prompt:?}");
        }
        assert!(password_round(
            "jdoe@corp.local",
            "(jdoe@corp.local@host) Password: "
        ));
        assert!(password_round(r"CORP\jdoe", r"Password for CORP\jdoe: "));
        assert!(password_round("admin", "admin@host's password: "));
        assert!(password_round("admin", "Password for admin:"));
    }

    #[test]
    fn a_one_time_code_is_never_the_password_even_named_a_password() {
        for prompt in [
            "Verification code: ",
            "One-time password (OATH) for `jdoe':",
            "One time password: ",
            "OneTimePassword: ",
            "OTP: ",
            "OTP password: ",
            "OTPpassword: ",
            "OTPPassword: ",
            "Password + OTP: ",
            "Password and token: ",
            "Enter PASSCODE: ",
            "PIN: ",
            "Password or PIN: ",
            "Token password: ",
            "Tokenpasswort: ",
            "2FA password: ",
            "MFA password: ",
            "Second factor password: ",
            "Second password: ",
            "2nd password: ",
            "Two-step password: ",
            "Authenticator password: ",
            "RSA SecurID password: ",
            "RADIUS password: ",
            "Dynamic password: ",
            "Temporary password: ",
            "Enable password: ",
            "Key passphrase: ",
            "Passphrase: ",
            "Code de vérification : ",
            "Mot de passe à usage unique : ",
            "Mot de passe unique : ",
            "Mot de passe du jeton : ",
            "Mot de passe A2F : ",
            "Einmalpasswort: ",
            "Bestätigungscode: ",
            "Contraseña de un solo uso: ",
            "Contraseña temporal: ",
            "Contrasena temporal: ",
            "Código de verificación: ",
            "Password:\nOTP:",
        ] {
            assert!(!password_round(USER, prompt), "{prompt:?}");
        }
    }

    #[test]
    fn a_password_change_is_never_answered() {
        for prompt in [
            "New password: ",
            "Retype new password: ",
            "Re-enter password: ",
            "Confirm password: ",
            "Password again: ",
            "(current) UNIX password: ",
            "Current password: ",
            "Old password: ",
            "Nouveau mot de passe : ",
            "Retapez le mot de passe : ",
            "Neues Passwort: ",
            "Nueva contraseña: ",
        ] {
            assert!(!password_round(USER, prompt), "{prompt:?}");
        }
    }

    #[test]
    fn a_prompt_naming_another_account_is_never_answered() {
        for prompt in [
            "(admin@host) Password: ",
            "(jdoe@corp.local@host) Password: ",
            "root@host's password: ",
            "xjdoe@host's password: ",
            "jdoe2@host's password: ",
            "Root password: ",
            "admin's password: ",
            "Enter admin password: ",
            "Password for admin:",
            "Password for `admin':",
            "Password for:",
            "Mot de passe pour admin :",
            "Mot de passe de l'administrateur :",
            "Passwort für admin:",
            "Passwort fur admin:",
            "Kennwort für admin:",
            "Kennwort fur admin:",
            "Contraseña para admin:",
            "Contrasena para admin:",
            "Contraseña de admin:",
            "Contrasena de admin:",
            "'s password: ",
            r"Password for CORP\jdoe: ",
        ] {
            assert!(!password_round(USER, prompt), "{prompt:?}");
        }
    }

    #[test]
    fn a_look_alike_or_a_split_word_is_a_doubt() {
        for prompt in [
            // Cyrillic "р" and "а".
            "\u{440}\u{430}ssword: ",
            // Full-width letters.
            "\u{ff30}\u{ff41}\u{ff53}\u{ff53}\u{ff57}\u{ff4f}\u{ff52}\u{ff44}: ",
            // A zero-width space inside the word.
            "Pass\u{200b}word: ",
            // A decomposed "ñ" splits the word.
            "Contrasen\u{303}a: ",
        ] {
            assert!(!password_round(USER, prompt), "{prompt:?}");
        }
    }

    #[test]
    fn a_shown_answer_several_prompts_or_no_password_wording_is_never_answered() {
        assert!(!asks_for_the_account_password(&round(
            USER,
            "",
            &[("Password: ", true)]
        )));
        assert!(!asks_for_the_account_password(&round(
            USER,
            "",
            &[("Password: ", false), ("Password: ", false)]
        )));
        assert!(!asks_for_the_account_password(&round(USER, "", &[])));
        for prompt in [
            "",
            "   ",
            ": ",
            "Username: ",
            "Response: ",
            "Enter your answer:",
            "jdoe@host's: ",
        ] {
            assert!(!password_round(USER, prompt), "{prompt:?}");
        }
    }

    /// Whether a "Password: " round titled `name` with `instructions` is the password.
    fn with_notice(user: &str, name: &str, instructions: &str) -> bool {
        let mut asked = round(user, instructions, &[("Password: ", false)]);
        asked.name = name.to_owned();
        asks_for_the_account_password(&asked)
    }

    #[test]
    fn a_title_or_instructions_naming_a_factor_or_an_account_make_the_round_the_user_s() {
        for said in [
            "Enter the one-time password shown by your token.",
            "Duo two-factor login",
            "Enter your RSA SecurID tokencode",
            "Enter your OneTimePassword",
            "OTPpassword required",
            "Enter your second password",
            "Temporary password",
            "Zweifaktorauthentifizierung",
            "Enter the password for admin@db01",
            "Enter the password for admin",
            "Root password required",
            "admin's password",
            "Code de vérification",
            "Mot de passe à usage unique",
            "Contraseña temporal",
            // A PAM notice: the user reads it in the keyboard-interactive dialog.
            "Your password will expire in 5 days.",
            "Last login: Mon Oct 12 10:00:00 2026 from 10.0.0.1",
        ] {
            assert!(!with_notice(USER, "", said), "instructions {said:?}");
            assert!(!with_notice(USER, said, ""), "title {said:?}");
        }
    }

    #[test]
    fn a_plain_title_or_instructions_keep_the_round_the_password() {
        for said in [
            "",
            "Please enter your password",
            "Please enter your password to continue.",
            "Password authentication",
            "keyboard-interactive",
            "Enter the password for jdoe",
            "Enter the password for jdoe@CORP.LOCAL",
            "Veuillez saisir votre mot de passe",
            "Bitte geben Sie Ihr Passwort ein",
            "Ingrese su contraseña",
        ] {
            assert!(with_notice(USER, "", said), "instructions {said:?}");
            assert!(with_notice(USER, said, ""), "title {said:?}");
        }
    }

    #[test]
    fn a_user_name_that_is_a_word_counts_only_where_it_stands_as_an_account() {
        // Users "token", "otp", "new" and "enable": "Token password:" names a token.
        for (user, prompt) in [
            ("token", "Token password: "),
            ("otp", "OTP password: "),
            ("new", "New password: "),
            ("enable", "Enable password: "),
            ("second", "Second password: "),
            (USER, "jdoe password: "),
            (USER, "Password jdoe: "),
        ] {
            assert!(!password_round(user, prompt), "{user:?} {prompt:?}");
        }
        for (user, prompt) in [
            ("token", "Password for token: "),
            ("token", "(token@host) Password: "),
            ("otp", "otp@host's password: "),
            ("new", "Mot de passe de new : "),
            (USER, "jdoe's password: "),
        ] {
            assert!(password_round(user, prompt), "{user:?} {prompt:?}");
        }
    }

    #[test]
    fn a_host_label_naming_a_factor_or_hiding_another_account_is_a_doubt() {
        for prompt in [
            "(jdoe@otp) Password: ",
            "(jdoe@token-host) Password: ",
            "(jdoe@radius01) Password: ",
            "jdoe@host:root's password: ",
            "(jdoe@host:admin) Password: ",
            "(jdoe@[fe80::1) Password: ",
        ] {
            assert!(!password_round(USER, prompt), "{prompt:?}");
        }
        for prompt in [
            "(jdoe@host) Password: ",
            "(jdoe@[fe80::1%eth0]) Password: ",
            "Password for jdoe@CORP.LOCAL: ",
        ] {
            assert!(password_round(USER, prompt), "{prompt:?}");
        }
    }

    #[test]
    fn only_the_first_password_question_of_a_connection_is_the_password() {
        // C# `MarkPasswordSpent`: once a password was asked on this connection, by either
        // method, a later round worded as a password is the user's.
        let plain = round(USER, "", &[("Password: ", false)]);
        assert!(round_is_the_password(0, &plain));
        assert!(!round_is_the_password(1, &plain));
        assert!(!round_is_the_password(2, &plain));
        let code = round(USER, "", &[("Verification code: ", false)]);
        assert!(!round_is_the_password(0, &code));
    }
}
