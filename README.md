# Photo Oxide
[![Build Status](https://travis-ci.org/TheBiggerGuy/photooxide.svg?branch=master)](https://travis-ci.org/TheBiggerGuy/photooxide)
[![codecov](https://codecov.io/gh/TheBiggerGuy/photooxide/branch/master/graph/badge.svg)](https://codecov.io/gh/TheBiggerGuy/photooxide)
[![](https://img.shields.io/github/issues-raw/TheBiggerGuy/photooxide.svg)](https://github.com/TheBiggerGuy/photooxide/issues)
[![](https://tokei.rs/b1/github/TheBiggerGuy/photooxide)](https://github.com/TheBiggerGuy/photooxide).

A Google Photos FUSE Filesystem

## Features
* Image and video support
* Folder per album
* Local DB for fast listing

# Development

## Test running
```bash
mkdir -p photo_mount; fusermount -u photo_mount; cargo run -- photo_mount; fusermount -u photo_mount
```
