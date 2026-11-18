//! The SSH agent protocol (draft-miller-ssh-agent): the framing, and the messages the relay reads. The relay passes
//! every message on unchanged; it only reads sign requests (to decide), `session-bind@openssh.com` (the host and
//! whether the connection is a forwarding) and the answers to the key list (the keys' comments).

use std::io::{self, Read};

/// The longest message body accepted (OpenSSH's agent uses the same limit); a longer one closes the connection.
pub const MAX_MESSAGE: usize = 256 * 1024;

/// `SSH_AGENT_FAILURE`
pub const FAILURE: u8 = 5;
/// `SSH_AGENT_SUCCESS`
pub const SUCCESS: u8 = 6;
/// `SSH_AGENTC_REQUEST_IDENTITIES`
pub const REQUEST_IDENTITIES: u8 = 11;
/// `SSH_AGENT_IDENTITIES_ANSWER`
pub const IDENTITIES_ANSWER: u8 = 12;
/// `SSH_AGENTC_SIGN_REQUEST`
pub const SIGN_REQUEST: u8 = 13;
/// `SSH_AGENT_SIGN_RESPONSE`
pub const SIGN_RESPONSE: u8 = 14;
/// `SSH_AGENTC_EXTENSION`
pub const EXTENSION: u8 = 27;
/// `SSH_AGENT_EXTENSION_FAILURE`
pub const EXTENSION_FAILURE: u8 = 28;
/// The extension OpenSSH (8.9 and later) sends before it uses a connection for a host.
pub const SESSION_BIND: &str = "session-bind@openssh.com";

/// Reads one message and returns its body (the type byte and what follows). `None` at the end of the stream before
/// a message starts; `InvalidData` for a zero length or one above [`MAX_MESSAGE`] (the body is not read).
pub fn read_message(r: &mut impl Read) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 4];
    let mut got = 0;
    while got < header.len() {
        match r.read(&mut header[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => got += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    let len = u32::from_be_bytes(header) as usize;
    if len == 0 || len > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("an agent message of {len} bytes"),
        ));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body)?;
    Ok(Some(body))
}

/// A body with its 32-bit length in front.
pub fn frame(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// `SSH_AGENT_FAILURE`, framed.
pub fn failure() -> Vec<u8> {
    frame(&[FAILURE])
}

/// Reads the fields of a message body in order; every read fails past the end.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(head)
    }

    fn byte(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// A sign request: the public key blob, the data to sign and the flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignRequest {
    pub key: Vec<u8>,
    pub data: Vec<u8>,
    pub flags: u32,
}

/// The parts of a sign request body, when it is one.
pub fn sign_request(body: &[u8]) -> Option<SignRequest> {
    let mut c = Cursor(body);
    if c.byte()? != SIGN_REQUEST {
        return None;
    }
    Some(SignRequest {
        key: c.string()?.to_vec(),
        data: c.string()?.to_vec(),
        flags: c.u32()?,
    })
}

/// What `session-bind@openssh.com` says: the server's host key, and whether the connection carries requests forwarded
/// from that host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionBind {
    pub host_key: Vec<u8>,
    pub forwarding: bool,
}

/// The host key and the forwarding flag of a `session-bind@openssh.com` extension body (`None` for anything else).
pub fn session_bind(body: &[u8]) -> Option<SessionBind> {
    let mut c = Cursor(body);
    if c.byte()? != EXTENSION || c.string()? != SESSION_BIND.as_bytes() {
        return None;
    }
    let host_key = c.string()?.to_vec();
    let _session_id = c.string()?;
    let _signature = c.string()?;
    let forwarding = c.byte()? != 0;
    Some(SessionBind {
        host_key,
        forwarding,
    })
}

/// The keys of an identities answer: each key blob and its comment (the name shown in 1Password).
pub fn identities(body: &[u8]) -> Option<Vec<(Vec<u8>, String)>> {
    let mut c = Cursor(body);
    if c.byte()? != IDENTITIES_ANSWER {
        return None;
    }
    let n = c.u32()?;
    let mut keys = Vec::new();
    for _ in 0..n {
        let blob = c.string()?.to_vec();
        keys.push((blob, text(c.string()?)));
    }
    Some(keys)
}

/// What a signature is for, read from the data to sign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Purpose {
    /// User authentication (RFC 4252 §7), as this user on the server.
    UserAuth { user: String },
    /// An SSHSIG signature (`ssh-keygen -Y sign`, git's commit signing) in this namespace.
    Sshsig { namespace: String },
    /// Anything else.
    Other,
}

/// What the data of a sign request is for.
pub fn purpose(data: &[u8]) -> Purpose {
    sshsig(data)
        .or_else(|| userauth(data))
        .unwrap_or(Purpose::Other)
}

/// The SSHSIG blob: the magic, then the namespace.
fn sshsig(data: &[u8]) -> Option<Purpose> {
    let rest = data.strip_prefix(b"SSHSIG")?;
    let namespace = text(Cursor(rest).string()?);
    Some(Purpose::Sshsig { namespace })
}

/// RFC 4252 §7: the session identifier, `SSH_MSG_USERAUTH_REQUEST` (50), the user, the service and a `publickey…`
/// method.
fn userauth(data: &[u8]) -> Option<Purpose> {
    let mut c = Cursor(data);
    let _session_id = c.string()?;
    if c.byte()? != 50 {
        return None;
    }
    let user = text(c.string()?);
    let _service = c.string()?;
    if !c.string()?.starts_with(b"publickey") {
        return None;
    }
    Some(Purpose::UserAuth { user })
}

/// The fingerprint of a key blob as `ssh-keygen -l` prints it: `SHA256:` and the digest in base64 without padding.
pub fn fingerprint(blob: &[u8]) -> String {
    let b64 = crate::base64::encode(&crate::sha256::digest(blob));
    format!("SHA256:{}", b64.trim_end_matches('='))
}

/// The key type at the start of a key blob (`ssh-ed25519`, `ssh-rsa`, …).
pub fn key_type(blob: &[u8]) -> Option<String> {
    let name = Cursor(blob).string()?;
    (!name.is_empty() && name.iter().all(|b| b.is_ascii_graphic())).then(|| text(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An SSH string: a 32-bit length and the bytes.
    fn s(b: &[u8]) -> Vec<u8> {
        let mut out = (b.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(b);
        out
    }

    fn cat(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    /// Hands out one byte per read, as a pipe may.
    struct Trickle(Vec<u8>, usize);

    impl Read for Trickle {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.1 >= self.0.len() || buf.is_empty() {
                return Ok(0);
            }
            buf[0] = self.0[self.1];
            self.1 += 1;
            Ok(1)
        }
    }

    const KEY_B64: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIJIdrbVVUhN/Pw7374GtdFg/XdjLsCfjFXLmmQn9U8ff";

    fn key() -> Vec<u8> {
        crate::base64::decode(KEY_B64).unwrap()
    }

    #[test]
    fn reads_one_message_at_a_time_from_a_trickle() {
        let mut stream = frame(&[REQUEST_IDENTITIES]);
        stream.extend(frame(&cat(&[
            &[SIGN_REQUEST],
            &s(b"k"),
            &s(b"d"),
            &[0, 0, 0, 2],
        ])));
        assert_eq!(&stream[..5], &[0, 0, 0, 1, REQUEST_IDENTITIES]);
        let mut r = Trickle(stream, 0);
        assert_eq!(
            read_message(&mut r).unwrap(),
            Some(vec![REQUEST_IDENTITIES])
        );
        let second = read_message(&mut r).unwrap().unwrap();
        assert_eq!(second[0], SIGN_REQUEST);
        assert_eq!(second.len(), 1 + 5 + 5 + 4);
        assert_eq!(read_message(&mut r).unwrap(), None, "the end of the stream");
        assert_eq!(failure(), vec![0, 0, 0, 1, FAILURE]);
    }

    #[test]
    fn a_zero_length_and_one_above_the_limit_are_refused() {
        let zero = read_message(&mut Trickle(vec![0, 0, 0, 0], 0)).unwrap_err();
        assert_eq!(zero.kind(), io::ErrorKind::InvalidData);
        let mut at_limit = (MAX_MESSAGE as u32).to_be_bytes().to_vec();
        at_limit.extend(vec![7u8; MAX_MESSAGE]);
        assert_eq!(
            read_message(&mut at_limit.as_slice())
                .unwrap()
                .map(|b| b.len()),
            Some(MAX_MESSAGE)
        );
        // the header alone: refused before the body is read
        let over = ((MAX_MESSAGE + 1) as u32).to_be_bytes();
        let err = read_message(&mut over.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn the_end_inside_a_message_is_an_error() {
        let err = read_message(&mut Trickle(vec![0, 0, 0, 10, 1, 2, 3], 0)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        let err = read_message(&mut Trickle(vec![0, 0], 0)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn the_parts_of_a_sign_request() {
        let body = cat(&[&[SIGN_REQUEST], &s(&key()), &s(b"to sign"), &[0, 0, 0, 4]]);
        assert_eq!(
            sign_request(&body),
            Some(SignRequest {
                key: key(),
                data: b"to sign".to_vec(),
                flags: 4,
            })
        );
        assert_eq!(sign_request(&body[..body.len() - 1]), None, "no flags");
        assert_eq!(sign_request(&[REQUEST_IDENTITIES]), None);
        assert_eq!(sign_request(&[]), None);
    }

    #[test]
    fn session_bind_with_and_without_forwarding() {
        let bind = |fwd: u8| {
            cat(&[
                &[EXTENSION],
                &s(SESSION_BIND.as_bytes()),
                &s(&key()),
                &s(b"session id"),
                &s(b"signature"),
                &[fwd],
            ])
        };
        assert_eq!(
            session_bind(&bind(0)),
            Some(SessionBind {
                host_key: key(),
                forwarding: false,
            })
        );
        assert_eq!(session_bind(&bind(1)).map(|b| b.forwarding), Some(true));
        let other = cat(&[&[EXTENSION], &s(b"query"), &s(b"x")]);
        assert_eq!(session_bind(&other), None, "another extension");
        let short = bind(1);
        assert_eq!(session_bind(&short[..short.len() - 1]), None, "no flag");
        assert_eq!(session_bind(&[SIGN_REQUEST]), None);
    }

    #[test]
    fn the_keys_of_an_identities_answer() {
        let body = cat(&[
            &[IDENTITIES_ANSWER],
            &3u32.to_be_bytes(),
            &s(&key()),
            &s("GitHub 用".as_bytes()),
            &s(b"blob2"),
            &s(b""),
            &s(b"blob3"),
            &s(&[0xff, b'x']),
        ]);
        let keys = identities(&body).unwrap();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], (key(), "GitHub 用".to_owned()));
        assert_eq!(keys[1].1, "");
        assert_eq!(keys[2].1, "\u{fffd}x", "a comment that is not UTF-8");
        let mut lying = body.clone();
        lying[4] = 4;
        assert_eq!(identities(&lying), None, "fewer keys than it says");
        assert_eq!(identities(&[IDENTITIES_ANSWER, 0, 0, 0, 0]), Some(vec![]));
        assert_eq!(identities(&[FAILURE]), None);
    }

    #[test]
    fn a_key_list_with_many_keys_and_a_count_that_lies() {
        let mut body = vec![IDENTITIES_ANSWER];
        body.extend_from_slice(&500u32.to_be_bytes());
        for i in 0..500u32 {
            body.extend(s(&i.to_be_bytes()));
            body.extend(s(format!("key {i}").as_bytes()));
        }
        let keys = identities(&body).unwrap();
        assert_eq!(keys.len(), 500);
        assert_eq!(keys[499].1, "key 499");
        let mut huge = vec![IDENTITIES_ANSWER];
        huge.extend_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            identities(&huge),
            None,
            "a count far above what the body holds"
        );
    }

    fn userauth(method: &str, extra: &[u8]) -> Vec<u8> {
        cat(&[
            &s(b"session id"),
            &[50],
            &s(b"git"),
            &s(b"ssh-connection"),
            &s(method.as_bytes()),
            &[1],
            &s(b"ssh-ed25519"),
            &s(&key()),
            extra,
        ])
    }

    #[test]
    fn user_authentication_names_the_user() {
        assert_eq!(
            purpose(&userauth("publickey", b"")),
            Purpose::UserAuth { user: "git".into() }
        );
        assert_eq!(
            purpose(&userauth(
                "publickey-hostbound-v00@openssh.com",
                &s(b"host key")
            )),
            Purpose::UserAuth { user: "git".into() }
        );
    }

    #[test]
    fn sshsig_names_the_namespace() {
        let data = cat(&[
            b"SSHSIG",
            &s(b"git"),
            &s(b""),
            &s(b"sha512"),
            &s(&[1u8; 64]),
        ]);
        assert_eq!(
            purpose(&data),
            Purpose::Sshsig {
                namespace: "git".into()
            }
        );
    }

    #[test]
    fn anything_else_is_other() {
        assert_eq!(purpose(b""), Purpose::Other);
        assert_eq!(purpose(b"random bytes that are no request"), Purpose::Other);
        // a userauth shape with another message number
        let mut data = userauth("publickey", b"");
        data[4 + 10] = 51;
        assert_eq!(purpose(&data), Purpose::Other);
        assert_eq!(purpose(b"SSHSIG"), Purpose::Other, "no namespace");
    }

    #[test]
    fn fingerprints_like_ssh_keygen() {
        // ssh-keygen -lf of a key made for this test (its private half was thrown away)
        assert_eq!(
            fingerprint(&key()),
            "SHA256:ylAg/YojegDgH4I6h4lafZQUAs401hI3a3JtvWR6U+s"
        );
        assert_eq!(key_type(&key()).as_deref(), Some("ssh-ed25519"));
        assert_eq!(key_type(b"\x00\x00"), None);
    }
}
