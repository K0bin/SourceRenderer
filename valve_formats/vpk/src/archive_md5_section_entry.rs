pub struct ArchiveMD5SectionEntry {
    /// The CRC32 checksum of this entry
    pub archive_index: u32,

    /// The Offset in the package
    pub offset: u32,

    /// The length in bytes
    pub length: u32,

    /// The expected Checksum checksum
    pub checksum: [u8; 16],
}
