pub trait ReadAllOnce {
    fn read_all(self, buf: &mut Vec<u8>) -> std::io::Result<usize>;
}
