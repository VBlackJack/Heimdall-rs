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
//! plain password prompt is made of, and no one-time code in the round's title or
//! instructions.
//!
//! A round wrongly taken for a password sends the secret to the wrong factor, which cannot
//! be undone; a password round missed only asks the user. So the prompt is matched against
//! what it may say, never against what it may not: a word outside the list ("Temporary",
//! "RADIUS", "admin", "Einmalpasswort") is a doubt, and every doubt is a "no".

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

/// Words after which a prompt names the account it asks about ("Password for jdoe:"):
/// allowed only before the user's own account.
const ACCOUNT_INTRODUCERS: &[&str] = &["for", "pour", "für", "fur", "para"];

/// The possessive of "jdoe@host's password": allowed only after the user's own account.
const POSSESSIVE: &str = "s";

/// Whole words that name a one-time code or a second factor in a round's title or
/// instructions (C# `OneTimeCodePromptPattern`, widened). The prompt itself needs no such
/// list: none of these words is a plain password prompt's.
const ONE_TIME_CODE_WORDS: &[&str] = &[
    // English.
    "otp",
    "totp",
    "hotp",
    "oath",
    "onetime",
    "code",
    "codes",
    "passcode",
    "token",
    "tokens",
    "verification",
    "verify",
    "pin",
    "2fa",
    "mfa",
    "tfa",
    "factor",
    "factors",
    "authenticator",
    "yubikey",
    "duo",
    "sms",
    // French.
    "jeton",
    "jetons",
    "vérification",
    "facteur",
    "facteurs",
    "a2f",
    "authentificateur",
    // German.
    "bestätigungscode",
    "bestatigungscode",
    "verifizierungscode",
    "faktor",
    // Spanish.
    "código",
    "codigo",
    "verificación",
    "verificacion",
];

/// Words that name a one-time code by their beginning: German compounds ("Einmalpasswort",
/// "Einmalkennwort"), as the C# `einmal\w*`.
const ONE_TIME_CODE_PREFIXES: &[&str] = &["einmal"];

/// Consecutive words that name a one-time code or a second factor.
const ONE_TIME_CODE_PHRASES: &[&[&str]] = &[
    &["one", "time"],
    &["two", "step"],
    &["2", "step"],
    &["usage", "unique"],
    &["deux", "étapes"],
    &["un", "solo", "uso"],
];

/// What separates a host from the account in front of it ("jdoe@host").
const AT: char = '@';

/// A piece of a prompt: a word, or the user's own account (with the host after it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Word(&'a str),
    Own,
}

/// Characters an account name is made of, domain and machine accounts included
/// (`CORP\jdoe`, `host$`): the user's own name inside a longer one is not the user's.
fn is_account_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '\\' | '$' | AT)
}

/// Characters of the host label after "jdoe@", an IPv6 address included.
fn is_host_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '[' | ']' | '%')
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
/// round whose title and instructions name no one-time code.
pub(crate) fn asks_for_the_account_password(question: &KeyboardInteractiveQuestion) -> bool {
    let [prompt] = question.prompts.as_slice() else {
        return false;
    };
    if prompt.echo {
        return false;
    }
    let text = prompt.text.to_lowercase();
    let username = question.username.to_lowercase();
    let Some(tokens) = tokens(&text, &username) else {
        return false;
    };
    if !is_a_plain_password_prompt(&tokens) {
        return false;
    }
    [&question.name, &question.instructions]
        .into_iter()
        .all(|said| !names_a_one_time_code(&words(&said.to_lowercase())))
}

/// `true` when every token is allowed where it stands and one names a password.
fn is_a_plain_password_prompt(tokens: &[Token<'_>]) -> bool {
    let words: Vec<&str> = tokens
        .iter()
        .filter_map(|token| match token {
            Token::Word(word) => Some(*word),
            Token::Own => None,
        })
        .collect();
    let names_a_password = words.iter().any(|word| PASSWORD_WORDS.contains(word))
        || PASSWORD_PHRASES
            .iter()
            .any(|phrase| words.windows(phrase.len()).any(|window| window == *phrase));
    if !names_a_password {
        return false;
    }
    tokens.iter().enumerate().all(|(index, token)| {
        let Token::Word(word) = token else {
            return true;
        };
        let after_own = index > 0 && tokens[index - 1] == Token::Own;
        let before_own = tokens.get(index + 1) == Some(&Token::Own);
        if ACCOUNT_INTRODUCERS.contains(word) {
            return before_own;
        }
        if *word == POSSESSIVE {
            return after_own;
        }
        PASSWORD_WORDS.contains(word) || PLAIN_WORDS.contains(word)
    })
}

/// The tokens of lowercased `prompt`, the whole-name occurrences of lowercased `username`
/// (with the `@host` after one) taken as [`Token::Own`]. `None` when an `@` is left: another
/// account ("(admin@host) Password:"), or a doubt.
fn tokens<'a>(prompt: &'a str, username: &str) -> Option<Vec<Token<'a>>> {
    let mut tokens = Vec::new();
    let mut copied = 0;
    if !username.is_empty() {
        for (start, found) in prompt.match_indices(username) {
            if start < copied {
                continue;
            }
            let mut end = start + found.len();
            let before = prompt[..start].chars().next_back();
            let after = prompt[end..].chars().next();
            let whole_name = !before.is_some_and(is_account_char)
                && !after.is_some_and(|c| c != AT && is_account_char(c));
            if !whole_name {
                continue;
            }
            if after == Some(AT) {
                end += AT.len_utf8();
                end += prompt[end..]
                    .chars()
                    .take_while(|&c| is_host_char(c))
                    .map(char::len_utf8)
                    .sum::<usize>();
            }
            plain_tokens(&prompt[copied..start], &mut tokens)?;
            tokens.push(Token::Own);
            copied = end;
        }
    }
    plain_tokens(&prompt[copied..], &mut tokens)?;
    Some(tokens)
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

fn names_a_one_time_code(words: &[&str]) -> bool {
    words.iter().any(|word| ONE_TIME_CODE_WORDS.contains(word))
        || ONE_TIME_CODE_PHRASES
            .iter()
            .any(|phrase| words.windows(phrase.len()).any(|window| window == *phrase))
        || words.iter().any(|word| {
            ONE_TIME_CODE_PREFIXES
                .iter()
                .any(|prefix| word.starts_with(prefix))
        })
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

    #[test]
    fn instructions_naming_a_one_time_code_make_the_round_a_factor_s() {
        assert!(!asks_for_the_account_password(&round(
            USER,
            "Enter the one-time password shown by your token.",
            &[("Password: ", false)]
        )));
        let mut titled = round(USER, "", &[("Password: ", false)]);
        titled.name = "Duo two-factor login".to_owned();
        assert!(!asks_for_the_account_password(&titled));
        // Other instructions do not change the answer.
        assert!(asks_for_the_account_password(&round(
            USER,
            "Your password will expire in 5 days.",
            &[("Password: ", false)]
        )));
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
