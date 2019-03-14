## Store cache in FUSE struct

FUSE has a unsigned long to store a pointer to private data to be used in open/opendir calls. Potentially use this to index into the Rust cached data.
https://github.com/libfuse/libfuse/wiki/FAQ#is-it-possible-to-store-a-pointer-to-private-data-in-the-fuse_file_info-structure
