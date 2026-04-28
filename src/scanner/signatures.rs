pub struct Signature {
    pub name: &'static str,
    pub magic: &'static [u8],
}

pub const SIGNATURES: &[Signature] = &[
    Signature { name: "squashfs", magic: b"hsqs" },
    Signature { name: "squashfs", magic: b"sqsh" },
    Signature { name: "squashfs", magic: b"shsq" },
    Signature { name: "squashfs", magic: b"qshs" },
    Signature { name: "jffs2", magic: b"\x85\x19" },
    Signature { name: "jffs2", magic: b"\x19\x85" },
    Signature { name: "ubifs", magic: b"\x31\x18\x10\x06" },
    Signature { name: "cramfs", magic: b"\x45\x3d\xcd\x28" },
    Signature { name: "cpio", magic: b"070701" },
    Signature { name: "cpio", magic: b"070702" },
    Signature { name: "gzip", magic: b"\x1f\x8b" },
    Signature { name: "zstd", magic: b"\x28\xb5\x2f\xfd" },
    Signature { name: "xz", magic: b"\xfd7zXZ" },
    Signature { name: "lzo", magic: b"\x89LZO" },
];

pub const SPECIAL_SIGNATURES: &[(&'static str, usize, &'static [u8])] = &[
    // name, offset relative to found, magic
    ("ext", 0x438, b"\x53\xef"),
];
