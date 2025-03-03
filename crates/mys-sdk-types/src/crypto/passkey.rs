use super::Secp256r1PublicKey;
use super::Secp256r1Signature;
use super::SimpleSignature;

/// An passkey authenticator with parsed fields. See field defition below. Can be initialized from [struct RawPasskeyAuthenticator].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasskeyAuthenticator {
    /// Compact r1 public key upon passkey creation.
    /// Initialized from `user_signature` in `RawPasskeyAuthenticator`.
    public_key: Secp256r1PublicKey,

    /// Normalized r1 signature returned by passkey.
    /// Initialized from `user_signature` in `RawPasskeyAuthenticator`.
    signature: Secp256r1Signature,

    /// Parsed challenge bytes from `client_data_json.challenge`.
    challenge: Vec<u8>,

    /// `authenticatorData` is a bytearray that encodes
    /// [Authenticator Data](https://www.w3.org/TR/webauthn-2/#sctn-authenticator-data)
    /// structure returned by the authenticator attestation
    /// response as is.
    authenticator_data: Vec<u8>,

    /// `clientDataJSON` contains a JSON-compatible
    /// UTF-8 encoded serialization of the client
    /// data which is passed to the authenticator by
    ///  the client during the authentication request
    /// (see [CollectedClientData](https://www.w3.org/TR/webauthn-2/#dictdef-collectedclientdata))
    client_data_json: String,
}

impl PasskeyAuthenticator {
    pub fn authenticator_data(&self) -> &[u8] {
        &self.authenticator_data
    }

    pub fn client_data_json(&self) -> &str {
        &self.client_data_json
    }

    pub fn challenge(&self) -> &[u8] {
        &self.challenge
    }

    pub fn signature(&self) -> SimpleSignature {
        SimpleSignature::Secp256r1 {
            signature: self.signature,
            public_key: self.public_key,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PasskeyPublicKey(Secp256r1PublicKey);

impl PasskeyPublicKey {
    pub fn inner(&self) -> &Secp256r1PublicKey {
        &self.0
    }
}

#[cfg(feature = "serde")]
#[cfg_attr(doc_cfg, doc(cfg(feature = "serde")))]
mod serialization {
    use crate::SignatureScheme;
    use crate::SimpleSignature;

    use super::*;
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serialize;
    use serde::Serializer;
    use serde_with::Bytes;
    use serde_with::DeserializeAs;
    use std::borrow::Cow;

    #[derive(serde::Serialize)]
    struct AuthenticatorRef<'a> {
        authenticator_data: &'a Vec<u8>,
        client_data_json: &'a String,
        signature: SimpleSignature,
    }

    #[derive(serde::Deserialize)]
    #[serde(rename = "PasskeyAuthenticator")]
    struct Authenticator {
        authenticator_data: Vec<u8>,
        client_data_json: String,
        signature: SimpleSignature,
    }

    impl Serialize for PasskeyAuthenticator {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            if serializer.is_human_readable() {
                let authenticator_ref = AuthenticatorRef {
                    authenticator_data: &self.authenticator_data,
                    client_data_json: &self.client_data_json,
                    signature: SimpleSignature::Secp256r1 {
                        signature: self.signature,
                        public_key: self.public_key,
                    },
                };

                authenticator_ref.serialize(serializer)
            } else {
                let bytes = self.to_bytes();
                serializer.serialize_bytes(&bytes)
            }
        }
    }

    impl<'de> Deserialize<'de> for PasskeyAuthenticator {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            if deserializer.is_human_readable() {
                let authenticator = Authenticator::deserialize(deserializer)?;
                Self::try_from_raw(authenticator)
            } else {
                let bytes: Cow<'de, [u8]> = Bytes::deserialize_as(deserializer)?;
                Self::from_serialized_bytes(bytes)
            }
        }
    }

    impl PasskeyAuthenticator {
        pub fn new(
            authenticator_data: Vec<u8>,
            client_data_json: String,
            signature: SimpleSignature,
        ) -> Option<Self> {
            Self::try_from_raw::<serde_json::Error>(Authenticator {
                authenticator_data,
                client_data_json,
                signature,
            })
            .ok()
        }

        fn try_from_raw<E: serde::de::Error>(
            Authenticator {
                authenticator_data,
                client_data_json,
                signature,
            }: Authenticator,
        ) -> Result<Self, E> {
            let SimpleSignature::Secp256r1 {
                signature,
                public_key,
            } = signature
            else {
                return Err(serde::de::Error::custom(
                    "expected passkey with secp256r1 signature",
                ));
            };

            let CollectedClientData {
                ty: _,
                challenge,
                origin: _,
            } = serde_json::from_str(&client_data_json).map_err(serde::de::Error::custom)?;

            // decode unpadded url endoded base64 data per spec:
            // https://w3c.github.io/webauthn/#base64url-encoding
            let challenge =
                <base64ct::Base64UrlUnpadded as base64ct::Encoding>::decode_vec(&challenge)
                    .map_err(|e| {
                        serde::de::Error::custom(format!(
                    "unable to decode base64urlunpadded into 3-byte intent and 32-byte digest: {e}"
                ))
                    })?;

            Ok(Self {
                public_key,
                signature,
                challenge,
                authenticator_data,
                client_data_json,
            })
        }

        pub(crate) fn from_serialized_bytes<T: AsRef<[u8]>, E: serde::de::Error>(
            bytes: T,
        ) -> Result<Self, E> {
            let bytes = bytes.as_ref();
            let flag = SignatureScheme::from_byte(
                *bytes
                    .first()
                    .ok_or_else(|| serde::de::Error::custom("missing signature scheme flag"))?,
            )
            .map_err(serde::de::Error::custom)?;
            if flag != SignatureScheme::Passkey {
                return Err(serde::de::Error::custom("invalid passkey flag"));
            }
            let bcs_bytes = &bytes[1..];

            let authenticator = bcs::from_bytes(bcs_bytes).map_err(serde::de::Error::custom)?;

            Self::try_from_raw(authenticator)
        }

        pub(crate) fn to_bytes(&self) -> Vec<u8> {
            let authenticator_ref = AuthenticatorRef {
                authenticator_data: &self.authenticator_data,
                client_data_json: &self.client_data_json,
                signature: SimpleSignature::Secp256r1 {
                    signature: self.signature,
                    public_key: self.public_key,
                },
            };

            let mut buf = Vec::new();
            buf.push(SignatureScheme::Passkey as u8);

            bcs::serialize_into(&mut buf, &authenticator_ref).expect("serialization cannot fail");
            buf
        }
    }

    /// The client data represents the contextual bindings of both the Relying Party and the client.
    /// It is a key-value mapping whose keys are strings. Values can be any type that has a valid
    /// encoding in JSON.
    ///
    /// > Note: The [`CollectedClientData`] may be extended in the future. Therefore it’s critical when
    /// >       parsing to be tolerant of unknown keys and of any reordering of the keys
    ///
    /// This struct conforms to the JSON byte serialization format expected of `CollectedClientData`,
    /// detailed in section [5.8.1.1 Serialization] of the WebAuthn spec. Namely the following
    /// requirements:
    ///
    /// * `type`, `challenge`, `origin`, `crossOrigin` must always be present in the serialized format
    ///   _in that order_.
    ///
    /// <https://w3c.github.io/webauthn/#dictionary-client-data>
    ///
    /// [5.8.1.1 Serialization]: https://w3c.github.io/webauthn/#clientdatajson-serialization
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CollectedClientData {
        /// This member contains the value [`ClientDataType::Create`] when creating new credentials, and
        /// [`ClientDataType::Get`] when getting an assertion from an existing credential. The purpose
        /// of this member is to prevent certain types of signature confusion attacks (where an attacker
        ///  substitutes one legitimate signature for another).
        #[serde(rename = "type")]
        pub ty: ClientDataType,

        /// This member contains the base64url encoding of the challenge provided by the Relying Party.
        /// See the [Cryptographic Challenges] security consideration.
        ///
        /// [Cryptographic Challenges]: https://w3c.github.io/webauthn/#sctn-cryptographic-challenges
        ///
        /// https://w3c.github.io/webauthn/#base64url-encoding
        ///
        /// The term Base64url Encoding refers to the base64 encoding using the URL- and filename-safe
        /// character set defined in Section 5 of [RFC4648], with all trailing '=' characters omitted
        /// (as permitted by Section 3.2) and without the inclusion of any line breaks, whitespace, or
        /// other additional characters.
        pub challenge: String,

        /// This member contains the fully qualified origin of the requester, as provided to the
        /// authenticator by the client, in the syntax defined by [RFC6454].
        ///
        /// [RFC6454]: https://www.rfc-editor.org/rfc/rfc6454
        pub origin: String,
        // /// This OPTIONAL member contains the inverse of the sameOriginWithAncestors argument value that
        // /// was passed into the internal method
        // #[serde(default, serialize_with = "truthiness")]
        // #[serde(rename = "type")]
        // pub cross_origin: Option<bool>,
    }

    /// Used to limit the values of [`CollectedClientData::ty`] and serializes to static strings.
    #[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
    pub enum ClientDataType {
        /// Serializes to the string `"webauthn.get"`
        ///
        /// Passkey's in MySocial only support the value `"webauthn.get"`, other values will be rejected.
        #[serde(rename = "webauthn.get")]
        Get,
        // /// Serializes to the string `"webauthn.create"`
        // #[serde(rename = "webauthn.create")]
        // Create,
        // /// Serializes to the string `"payment.get"`
        // /// This variant is part of the Secure Payment Confirmation specification
        // ///
        // /// See <https://www.w3.org/TR/secure-payment-confirmation/#client-extension-processing-authentication>
        // #[serde(rename = "payment.get")]
        // PaymentGet,
    }
}

#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for PasskeyAuthenticator {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        use proptest::collection::vec;
        use proptest::prelude::*;
        use serialization::ClientDataType;
        use serialization::CollectedClientData;

        (
            any::<Secp256r1PublicKey>(),
            any::<Secp256r1Signature>(),
            vec(any::<u8>(), 32),
            vec(any::<u8>(), 0..32),
        )
            .prop_map(
                |(public_key, signature, challenge_bytes, authenticator_data)| {
                    let challenge =
                        <base64ct::Base64UrlUnpadded as base64ct::Encoding>::encode_string(
                            &challenge_bytes,
                        );
                    let client_data_json = serde_json::to_string(&CollectedClientData {
                        ty: ClientDataType::Get,
                        challenge,
                        origin: "http://example.com".to_owned(),
                    })
                    .unwrap();

                    Self {
                        public_key,
                        signature,
                        challenge: challenge_bytes,
                        authenticator_data,
                        client_data_json,
                    }
                },
            )
            .boxed()
    }
}

#[cfg(test)]
mod tests {
    use crate::UserSignature;

    #[test]
    fn base64_encoded_passkey_user_signature() {
        let b64 = "BiVYDmenOnqS+thmz5m5SrZnWaKXZLVxgh+rri6LHXs25B0AAAAAnQF7InR5cGUiOiJ3ZWJhdXRobi5nZXQiLCAiY2hhbGxlbmdlIjoiQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQSIsIm9yaWdpbiI6Imh0dHA6Ly9sb2NhbGhvc3Q6NTE3MyIsImNyb3NzT3JpZ2luIjpmYWxzZSwgInVua25vd24iOiAidW5rbm93biJ9YgJMwqcOmZI7F/N+K5SMe4DRYCb4/cDWW68SFneSHoD2GxKKhksbpZ5rZpdrjSYABTCsFQQBpLORzTvbj4edWKd/AsEBeovrGvHR9Ku7critg6k7qvfFlPUngujXfEzXd8Eg";

        let sig = UserSignature::from_base64(b64).unwrap();
        assert!(matches!(sig, UserSignature::Passkey(_)));
    }
}
