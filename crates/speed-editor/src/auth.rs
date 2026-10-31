//! Host side of the Speed Editor challenge/response handshake.
//!
//! The device only sends input reports after the host has answered its
//! challenge. The algorithm and constants are ported from `bmd.py` by
//! Sylvain Munaut, Copyright (C) 2021 Sylvain Munaut <tnt@246tNt.com>,
//! licensed under the Apache License 2.0:
//! https://github.com/smunaut/blackmagic-misc/blob/master/bmd.py

const EVEN: [u64; 8] = [
    0x3ae1206f97c10bc8,
    0x2a9ab32bebf244c6,
    0x20a6f8b8df9adf0a,
    0xaf80ece52cfc1719,
    0xec2ee2f7414fd151,
    0xb055adfd73344a15,
    0xa63d2e3059001187,
    0x751bf623f42e0dde,
];

const ODD: [u64; 8] = [
    0x3e22b34f502e7fde,
    0x24656b981875ab1c,
    0xa17f3456df7bf8c3,
    0x6df72e1941aef698,
    0x72226f011e66ab94,
    0x3831a3c606296b42,
    0xfd7ff81881332c89,
    0x61a3f6474ff236c6,
];

const MASK: u64 = 0xa79a63f585d37bf0;

/// Feature report ID used for every step of the handshake.
pub const REPORT_ID: u8 = 6;

/// Computes the host's answer to the device challenge.
pub fn response(challenge: u64) -> u64 {
    let n = (challenge & 7) as usize;
    let mut v = challenge.rotate_right(8 * n as u32);
    let k = if (v & 1) == ((0x78 >> n) & 1) {
        EVEN[n]
    } else {
        v ^= v.rotate_right(8);
        ODD[n]
    };
    v ^ (v.rotate_right(8) & MASK) ^ k
}

#[cfg(test)]
mod tests {
    use super::*;

    // Generated with an independent Python implementation of bmd.py's
    // bmd_kbd_auth, covering both the even and odd branches.
    const VECTORS: &[(u64, u64)] = &[
        (0x0000000000000000, 0x3ae1206f97c10bc8),
        (0x0000000000000001, 0x2b9ab32bebf244c6),
        (0x0000000000000007, 0x751bf623f42e0ade),
        (0x0123456789abcdef, 0xe5c7b689e9967608),
        (0xfedcba9876543210, 0xc4a7da4761c769e8),
        (0xdeadbeefcafef00d, 0x6a04b6fcff2b4b21),
        (0x8000000000000005, 0x3831a3c603adeac2),
        (0xffffffffffffffff, 0x61a3f6474ff236c6),
    ];

    #[test]
    fn matches_reference() {
        for &(challenge, expected) in VECTORS {
            assert_eq!(response(challenge), expected, "challenge {challenge:#018x}");
        }
    }
}
