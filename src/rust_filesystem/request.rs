use fuse::Request;

pub trait UniqRequest {
    fn unique(&self) -> u64;
    fn uid(&self) -> u32;
    fn gid(&self) -> u32;
    fn pid(&self) -> u32;
}

impl<'a> UniqRequest for Request<'a> {
    fn unique(&self) -> u64 {
        self.unique()
    }
    fn uid(&self) -> u32 {
        self.uid()
    }
    fn gid(&self) -> u32 {
        self.gid()
    }
    fn pid(&self) -> u32 {
        self.pid()
    }
}
