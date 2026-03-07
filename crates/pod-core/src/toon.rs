use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use toon_format::{decode_default as decode_toon, encode_default as encode_toon};

#[derive(Serialize)]
struct ToonDocument<'a, T: Serialize> {
    document_type: &'static str,
    payload: &'a T,
}

#[derive(Deserialize)]
struct DecodedToonDocument<T> {
    document_type: String,
    payload: T,
}

/// Encode a serializable value using official TOON, with JSON fallback for
/// callers that still need a debuggable payload on encoding failure.
pub fn encode_toon_string<T: Serialize>(value: &T) -> String {
    encode_toon(value).unwrap_or_else(|_| {
        serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".to_string())
    })
}

/// Wrap a serializable payload in a lightweight typed document envelope and
/// encode it as official TOON.
pub fn encode_toon_document<T: Serialize>(document_type: &'static str, payload: &T) -> String {
    encode_toon_string(&ToonDocument {
        document_type,
        payload,
    })
}

/// Decode a TOON string into a typed payload.
pub fn decode_toon_string<T: DeserializeOwned>(document: &str) -> Result<T, String> {
    decode_toon(document).map_err(|error| error.to_string())
}

/// Decode a typed TOON document envelope and validate the expected document
/// kind before returning the payload.
pub fn decode_toon_document<T: DeserializeOwned>(
    document: &str,
    expected_document_type: &'static str,
) -> Result<T, String> {
    let decoded: DecodedToonDocument<T> = decode_toon_string(document)?;
    if decoded.document_type != expected_document_type {
        return Err(format!(
            "Expected TOON document type `{expected_document_type}`, got `{}`",
            decoded.document_type
        ));
    }
    Ok(decoded.payload)
}

/// Decode a TOON document into JSON for tests and tooling that want to assert
/// on structured content without depending on the TOON crate directly.
pub fn decode_toon_value(document: &str) -> Result<Value, String> {
    decode_toon(document).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::{decode_toon_document, decode_toon_value, encode_toon_document};

    #[derive(Serialize)]
    struct SamplePayload<'a> {
        label: &'a str,
        count: usize,
    }

    #[test]
    fn typed_documents_roundtrip_through_toon() {
        let document = encode_toon_document(
            "sample_payload",
            &SamplePayload {
                label: "monster",
                count: 2,
            },
        );

        let decoded = decode_toon_value(&document).expect("TOON document should decode");
        assert_eq!(decoded["document_type"], "sample_payload");
        assert_eq!(decoded["payload"]["label"], "monster");
        assert_eq!(decoded["payload"]["count"], 2);
    }

    #[test]
    fn typed_document_payloads_decode_with_expected_kind() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct SamplePayload {
            label: String,
            count: usize,
        }

        let document = encode_toon_document(
            "sample_payload",
            &serde_json::json!({
                "label": "monster",
                "count": 2,
            }),
        );

        let decoded: SamplePayload =
            decode_toon_document(&document, "sample_payload").expect("document should decode");
        assert_eq!(
            decoded,
            SamplePayload {
                label: "monster".to_string(),
                count: 2,
            }
        );
    }
}
