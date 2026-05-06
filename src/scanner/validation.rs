pub fn validate_squashfs(mmap: &[u8], offset: usize) -> bool {
    if offset + 96 > mmap.len() {
        return false;
    }
    
    let magic = &mmap[offset..offset + 4];
    let is_be = magic == b"sqsh" || magic == b"qshs";
    
    let read_u32 = |off: usize| -> u32 {
        let bytes: [u8; 4] = mmap[offset + off..offset + off + 4].try_into().unwrap();
        if is_be { u32::from_be_bytes(bytes) } else { u32::from_le_bytes(bytes) }
    };
    
    let read_u16 = |off: usize| -> u16 {
        let bytes: [u8; 2] = mmap[offset + off..offset + off + 2].try_into().unwrap();
        if is_be { u16::from_be_bytes(bytes) } else { u16::from_le_bytes(bytes) }
    };

    let inodes = read_u32(4);
    if inodes == 0 { return false; }
    
    let block_size = read_u32(12);
    if block_size < 4096 || block_size > 1048576 || !block_size.is_power_of_two() { return false; }
    
    let comp_id = read_u16(20);
    if comp_id == 0 || comp_id > 6 { return false; }
    
    let s_major = read_u16(28);
    if s_major != 4 { return false; }
    
    true
}

pub fn validate_jffs2(mmap: &[u8], offset: usize) -> bool {
    if offset + 12 > mmap.len() {
        return false;
    }
    
    let magic = &mmap[offset..offset + 2];
    let is_be = magic == b"\x19\x85"; // 0x1985 is big endian, 0x8519 is little endian
    
    let read_u16 = |off: usize| -> u16 {
        let bytes: [u8; 2] = mmap[offset + off..offset + off + 2].try_into().unwrap();
        if is_be { u16::from_be_bytes(bytes) } else { u16::from_le_bytes(bytes) }
    };
    
    let read_u32 = |off: usize| -> u32 {
        let bytes: [u8; 4] = mmap[offset + off..offset + off + 4].try_into().unwrap();
        if is_be { u32::from_be_bytes(bytes) } else { u32::from_le_bytes(bytes) }
    };

    let nodetype = read_u16(2);
    // 0x2000 = CLEANMARKER, 0x2001 = DIRENT, 0x2002 = INODE
    if nodetype != 0x2000 && nodetype != 0x2001 && nodetype != 0x2002 {
        return false;
    }
    
    let totlen = read_u32(4);
    if totlen < 12 {
        return false;
    }
    
    true
}

pub fn validate_ext(mmap: &[u8], offset: usize) -> bool {
    // magic is at 0x438, so superblock starts at 0x400 relative to offset
    let sb_offset = offset + 0x400;
    if sb_offset + 1024 > mmap.len() {
        return false;
    }
    
    let s_inodes_count = u32::from_le_bytes(mmap[sb_offset..sb_offset + 4].try_into().unwrap());
    if s_inodes_count == 0 {
        return false;
    }
    
    let s_blocks_count = u32::from_le_bytes(mmap[sb_offset + 4..sb_offset + 8].try_into().unwrap());
    if s_blocks_count == 0 {
        return false;
    }
    
    true
}
