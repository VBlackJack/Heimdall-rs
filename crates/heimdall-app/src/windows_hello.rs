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

//! Windows Hello asked before a connection, as the C# `WindowsHelloService`: whether it is
//! ready on this computer, then the user's verification, through the `WinRT`
//! `UserConsentVerifier`. The call sits behind [`ConsentVerifier`], so what its answers
//! come to is tested without a prompt.
//!
//! Each call waits on Windows: it runs on a worker thread, never the UI's, within a time
//! limit after which the request is cancelled. Only coarse outcomes are logged.

use std::time::Duration;

/// Whether this system has Windows Hello at all.
pub const SUPPORTED: bool = cfg!(windows);

/// Longest wait for Windows to say whether Windows Hello is ready.
pub const AVAILABILITY_TIME_LIMIT: Duration = Duration::from_secs(15);

/// Longest wait for the user's verification: the prompt is then cancelled, as the C# one
/// is when its connection is cancelled.
pub const VERIFICATION_TIME_LIMIT: Duration = Duration::from_secs(120);

/// Whether Windows Hello is ready, as `UserConsentVerifierAvailability` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// A verifier is present and the user is enrolled.
    Available,
    /// No biometric device or PIN provider.
    DeviceNotPresent,
    /// The user has not set Windows Hello up.
    NotConfiguredForUser,
    /// A group policy turns it off.
    DisabledByPolicy,
    /// The device is in use.
    DeviceBusy,
    /// Windows gave no answer: the call failed, took too long, or this is not Windows.
    Unknown,
}

/// What a verification came to, as `UserConsentVerificationResult` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verification {
    /// The user was verified.
    Verified,
    /// No biometric device or PIN provider.
    DeviceNotPresent,
    /// The user has not set Windows Hello up.
    NotConfiguredForUser,
    /// A group policy turns it off.
    DisabledByPolicy,
    /// The device is in use.
    DeviceBusy,
    /// Too many failed tries.
    RetriesExhausted,
    /// The user dismissed the prompt.
    Canceled,
    /// The call failed, or Windows answered with a value it did not have before.
    Failed,
    /// No answer within [`VERIFICATION_TIME_LIMIT`]: the request was cancelled.
    TimedOut,
}

/// The Windows Hello calls, put behind a trait as the C# `IWindowsHelloService`.
pub trait ConsentVerifier {
    /// Whether Windows Hello is ready on this computer.
    fn availability(&self) -> Availability;

    /// Asks the user to verify, `reason` shown in the prompt.
    fn verify(&self, reason: &str) -> Verification;
}

/// Why the connection gate refuses, each said as the C# says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelloRefusal {
    /// Windows Hello is unavailable or not set up: no prompt was raised.
    Unavailable,
    /// The verification did not succeed, the prompt dismissed included.
    NotVerified,
    /// The request was cancelled, here when it took too long.
    Cancelled,
}

/// Asks Windows Hello through `verifier`, as the C# gate asks it: unavailable refuses
/// without a prompt; anything but a verification refuses.
///
/// # Errors
///
/// Why the connection is refused.
pub fn ask(verifier: &impl ConsentVerifier, reason: &str) -> Result<(), HelloRefusal> {
    let availability = verifier.availability();
    if availability != Availability::Available {
        log::warn!("Windows Hello is required but unavailable ({availability:?})");
        return Err(HelloRefusal::Unavailable);
    }
    match verifier.verify(reason) {
        Verification::Verified => {
            log::info!("Windows Hello verification succeeded");
            Ok(())
        }
        Verification::TimedOut => {
            log::warn!("Windows Hello verification timed out and was cancelled");
            Err(HelloRefusal::Cancelled)
        }
        other => {
            log::info!("Windows Hello verification was not granted ({other:?})");
            Err(HelloRefusal::NotVerified)
        }
    }
}

/// This computer's Windows Hello.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemVerifier;

#[cfg(windows)]
impl ConsentVerifier for SystemVerifier {
    fn availability(&self) -> Availability {
        use windows::Security::Credentials::UI::{
            UserConsentVerifier, UserConsentVerifierAvailability as Answer,
        };

        let (sender, receiver) = std::sync::mpsc::channel();
        let started = UserConsentVerifier::CheckAvailabilityAsync().and_then(|operation| {
            operation.when(move |answer| {
                // The receiver gone means the wait is over: nothing to tell.
                sender.send(answer).ok();
            })?;
            Ok(operation)
        });
        let operation = match started {
            Ok(operation) => operation,
            Err(error) => {
                log::warn!("the Windows Hello availability check failed: {error}");
                return Availability::Unknown;
            }
        };
        match receiver.recv_timeout(AVAILABILITY_TIME_LIMIT) {
            Ok(Ok(answer)) => match answer {
                Answer::Available => Availability::Available,
                Answer::DeviceNotPresent => Availability::DeviceNotPresent,
                Answer::NotConfiguredForUser => Availability::NotConfiguredForUser,
                Answer::DisabledByPolicy => Availability::DisabledByPolicy,
                Answer::DeviceBusy => Availability::DeviceBusy,
                _ => Availability::Unknown,
            },
            Ok(Err(error)) => {
                log::warn!("the Windows Hello availability check failed: {error}");
                Availability::Unknown
            }
            Err(_) => {
                operation.Cancel().ok();
                log::warn!("the Windows Hello availability check took too long");
                Availability::Unknown
            }
        }
    }

    fn verify(&self, reason: &str) -> Verification {
        use windows::Security::Credentials::UI::{
            UserConsentVerificationResult as Answer, UserConsentVerifier,
        };
        use windows::core::HSTRING;

        let (sender, receiver) = std::sync::mpsc::channel();
        let started = UserConsentVerifier::RequestVerificationAsync(&HSTRING::from(reason))
            .and_then(|operation| {
                operation.when(move |answer| {
                    // The receiver gone means the wait is over: nothing to tell.
                    sender.send(answer).ok();
                })?;
                Ok(operation)
            });
        let operation = match started {
            Ok(operation) => operation,
            Err(error) => {
                log::warn!("the Windows Hello verification failed: {error}");
                return Verification::Failed;
            }
        };
        match receiver.recv_timeout(VERIFICATION_TIME_LIMIT) {
            Ok(Ok(answer)) => match answer {
                Answer::Verified => Verification::Verified,
                Answer::DeviceNotPresent => Verification::DeviceNotPresent,
                Answer::NotConfiguredForUser => Verification::NotConfiguredForUser,
                Answer::DisabledByPolicy => Verification::DisabledByPolicy,
                Answer::DeviceBusy => Verification::DeviceBusy,
                Answer::RetriesExhausted => Verification::RetriesExhausted,
                Answer::Canceled => Verification::Canceled,
                _ => Verification::Failed,
            },
            Ok(Err(error)) => {
                log::warn!("the Windows Hello verification failed: {error}");
                Verification::Failed
            }
            Err(_) => {
                operation.Cancel().ok();
                Verification::TimedOut
            }
        }
    }
}

/// Elsewhere there is no Windows Hello: the gate refuses, as it does on a Windows computer
/// without it.
#[cfg(not(windows))]
impl ConsentVerifier for SystemVerifier {
    fn availability(&self) -> Availability {
        Availability::Unknown
    }

    fn verify(&self, _reason: &str) -> Verification {
        Verification::Failed
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    /// Answers as told, counting the prompts it would have raised.
    struct Scripted {
        availability: Availability,
        verification: Verification,
        prompts: Cell<u32>,
    }

    impl Scripted {
        fn new(availability: Availability, verification: Verification) -> Self {
            Self {
                availability,
                verification,
                prompts: Cell::new(0),
            }
        }
    }

    impl ConsentVerifier for Scripted {
        fn availability(&self) -> Availability {
            self.availability
        }

        fn verify(&self, reason: &str) -> Verification {
            assert_eq!(reason, "why");
            self.prompts.set(self.prompts.get() + 1);
            self.verification
        }
    }

    #[test]
    fn anything_but_available_refuses_without_a_prompt() {
        for availability in [
            Availability::DeviceNotPresent,
            Availability::NotConfiguredForUser,
            Availability::DisabledByPolicy,
            Availability::DeviceBusy,
            Availability::Unknown,
        ] {
            let verifier = Scripted::new(availability, Verification::Verified);
            assert_eq!(
                ask(&verifier, "why"),
                Err(HelloRefusal::Unavailable),
                "{availability:?}"
            );
            assert_eq!(verifier.prompts.get(), 0, "{availability:?}");
        }
    }

    #[test]
    fn only_a_verification_lets_through_and_a_time_out_is_a_cancel() {
        let refused = Err(HelloRefusal::NotVerified);
        let cases = [
            (Verification::Verified, Ok(())),
            (Verification::DeviceNotPresent, refused),
            (Verification::NotConfiguredForUser, refused),
            (Verification::DisabledByPolicy, refused),
            (Verification::DeviceBusy, refused),
            (Verification::RetriesExhausted, refused),
            // The prompt dismissed is a failed verification, as the C# words it.
            (Verification::Canceled, refused),
            (Verification::Failed, refused),
            (Verification::TimedOut, Err(HelloRefusal::Cancelled)),
        ];
        for (verification, outcome) in cases {
            let verifier = Scripted::new(Availability::Available, verification);
            assert_eq!(ask(&verifier, "why"), outcome, "{verification:?}");
            assert_eq!(verifier.prompts.get(), 1, "{verification:?}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn elsewhere_windows_hello_is_unavailable() {
        assert_eq!(ask(&SystemVerifier, "why"), Err(HelloRefusal::Unavailable));
    }
}
