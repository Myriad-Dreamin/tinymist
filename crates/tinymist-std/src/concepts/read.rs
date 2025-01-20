/// A trait for reading all data from a source into a buffer.
pub trait ReadAllOnce {
    /// Reads all data from the source into the buffer.
    fn read_all(self, buf: &mut Vec<u8>) -> std::io::Result<usize>;
}
