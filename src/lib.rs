// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! # Meow Hasher
//!
//! An implementation of the [Meow hasher][meow-hasher] in native Rust providing
//! the [Digest][Digest] trait.
//!
//! The [Meow hasher][meow-hasher] is a hashing algorithm designed for hashing
//! large data sets (on the order of gigabytes) very efficiently. It takes about
//! 100 milliseconds to hash 1 gigabyte of data on an i7-7700 at 2.8GHz.
//!
//! It is *not* cryptographically secure.
//!
//! This implementation only supports the `x86_64` architecture, as it relies on
//! the AES hardware extensions of modern x86 CPUs.
//!
//! [meow-hasher]: https://mollyrocket.com/meowhash
//! [Digest]: https://docs.rs/digest/latest/digest/trait.Digest.html

#![no_std]

extern crate digest;

#[cfg(test)]
#[macro_use]
extern crate proptest;

use core::arch::x86_64::*;
use core::mem;
use core::ptr;
use core::slice;
use digest::generic_array::{
    typenum::{consts::*, Unsigned},
    GenericArray,
};
use digest::Digest;

#[derive(Clone, Copy)]
struct MeowLane {
    l0: __m128i,
    l1: __m128i,
    l2: __m128i,
    l3: __m128i,
}

impl MeowLane {
    fn new(seed: u128) -> Self {
        unsafe { mem::transmute([seed, seed, seed, seed]) }
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self as *const _ as *const u8, mem::size_of::<MeowLane>()) }
    }
}

#[inline]
unsafe fn aes_rotate(a: &mut MeowLane, b: &mut MeowLane) {
    a.l0 = _mm_aesdec_si128(a.l0, b.l0);
    a.l1 = _mm_aesdec_si128(a.l1, b.l1);
    a.l2 = _mm_aesdec_si128(a.l2, b.l2);
    a.l3 = _mm_aesdec_si128(a.l3, b.l3);

    let tmp = b.l0;
    b.l0 = b.l1;
    b.l1 = b.l2;
    b.l2 = b.l3;
    b.l3 = tmp;
}

#[inline]
#[cfg_attr(feature = "cargo-clippy", allow(clippy::cast_ptr_alignment))]
unsafe fn aes_load(s: &mut MeowLane, from: *const u8) {
    s.l0 = _mm_aesdec_si128(s.l0, ptr::read_unaligned(from as *const __m128i));
    s.l1 = _mm_aesdec_si128(
        s.l1,
        ptr::read_unaligned((from as *const __m128i).offset(1)),
    );
    s.l2 = _mm_aesdec_si128(
        s.l2,
        ptr::read_unaligned((from as *const __m128i).offset(2)),
    );
    s.l3 = _mm_aesdec_si128(
        s.l3,
        ptr::read_unaligned((from as *const __m128i).offset(3)),
    );
}

#[inline]
unsafe fn aes_merge(a: &mut MeowLane, b: &MeowLane) {
    a.l0 = _mm_aesdec_si128(a.l0, b.l0);
    a.l1 = _mm_aesdec_si128(a.l1, b.l1);
    a.l2 = _mm_aesdec_si128(a.l2, b.l2);
    a.l3 = _mm_aesdec_si128(a.l3, b.l3);
}

fn meow_hash_1(seed: u128, source: &[u8]) -> MeowLane {
    let mut len = source.len() as u64;

    unsafe {
        let iv = MeowLane::new(seed);

        let mut s0123 = iv;
        let mut s4567 = iv;
        let mut s89ab = iv;
        let mut scdef = iv;

        let mut block_count = len >> 8;
        len -= block_count << 8;
        let mut src_ptr = source.as_ptr();

        while block_count > 0 {
            aes_load(&mut s0123, src_ptr);
            src_ptr = src_ptr.add(mem::size_of::<MeowLane>());
            aes_load(&mut s4567, src_ptr);
            src_ptr = src_ptr.add(mem::size_of::<MeowLane>());
            aes_load(&mut s89ab, src_ptr);
            src_ptr = src_ptr.add(mem::size_of::<MeowLane>());
            aes_load(&mut scdef, src_ptr);
            src_ptr = src_ptr.add(mem::size_of::<MeowLane>());
            block_count -= 1;
        }

        if len > 0 {
            let partial = [iv, iv, iv, iv];
            let dest_ptr = partial.as_ptr() as *mut u8;
            ptr::copy_nonoverlapping(src_ptr, dest_ptr, len as usize);
            aes_merge(&mut s0123, &partial[0]);
            aes_merge(&mut s4567, &partial[1]);
            aes_merge(&mut s89ab, &partial[2]);
            aes_merge(&mut scdef, &partial[3]);
        }

        let mut r0 = iv;
        aes_rotate(&mut r0, &mut s0123);
        aes_rotate(&mut r0, &mut s4567);
        aes_rotate(&mut r0, &mut s89ab);
        aes_rotate(&mut r0, &mut scdef);

        aes_rotate(&mut r0, &mut s0123);
        aes_rotate(&mut r0, &mut s4567);
        aes_rotate(&mut r0, &mut s89ab);
        aes_rotate(&mut r0, &mut scdef);

        aes_rotate(&mut r0, &mut s0123);
        aes_rotate(&mut r0, &mut s4567);
        aes_rotate(&mut r0, &mut s89ab);
        aes_rotate(&mut r0, &mut scdef);

        aes_rotate(&mut r0, &mut s0123);
        aes_rotate(&mut r0, &mut s4567);
        aes_rotate(&mut r0, &mut s89ab);
        aes_rotate(&mut r0, &mut scdef);

        aes_merge(&mut r0, &iv);
        aes_merge(&mut r0, &iv);
        aes_merge(&mut r0, &iv);
        aes_merge(&mut r0, &iv);
        aes_merge(&mut r0, &iv);

        r0
    }
}

/// Meow hasher.
///
/// An implementation of the [Meow hasher][meow-hasher] providing the
/// [Digest][Digest] trait.
///
/// [meow-hasher]: https://mollyrocket.com/meowhash
/// [Digest]: https://docs.rs/digest/latest/digest/trait.Digest.html
pub struct MeowHasher {
    lanes: [MeowLane; 4],
    buf: [MeowLane; 4],
    index: usize,
    seed: u128,
}

impl Default for MeowHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl MeowHasher {
    /// Compute the hash of a chunk of data using the provided seed.
    ///
    /// This is a little faster than using `input` and `result`, because it
    /// doesn't have to keep an intermediate buffer.
    pub fn digest_with_seed(seed: u128, data: &[u8]) -> GenericArray<u8, U64> {
        GenericArray::clone_from_slice(meow_hash_1(seed, data).as_bytes())
    }

    /// Create a new hasher instance with the provided seed.
    pub fn with_seed(seed: u128) -> Self {
        MeowHasher {
            lanes: [
                MeowLane::new(seed),
                MeowLane::new(seed),
                MeowLane::new(seed),
                MeowLane::new(seed),
            ],
            buf: [
                MeowLane::new(seed),
                MeowLane::new(seed),
                MeowLane::new(seed),
                MeowLane::new(seed),
            ],
            index: 0,
            seed,
        }
    }

    #[inline]
    fn block(&self) -> [MeowLane; 4] {
        [
            MeowLane::new(self.seed),
            MeowLane::new(self.seed),
            MeowLane::new(self.seed),
            MeowLane::new(self.seed),
        ]
    }

    #[inline]
    fn block_size() -> usize {
        mem::size_of::<[MeowLane; 4]>()
    }

    #[inline]
    fn left(&self) -> usize {
        Self::block_size() - self.index
    }

    #[inline]
    unsafe fn buf_ptr(&mut self) -> *mut u8 {
        (self.buf.as_ptr() as *mut u8).add(self.index)
    }

    fn finalise(&mut self) -> MeowLane {
        let mut r0 = MeowLane::new(self.seed);
        let empty = MeowLane::new(self.seed);

        unsafe {
            if self.index > 0 {
                // Pad the last block if needed and merge it.
                let mut empty_block = self.block();
                let src_ptr = (&mut empty_block as *mut _ as *mut u8).add(self.index);
                let dest_ptr = self.buf_ptr();
                ptr::copy_nonoverlapping(src_ptr, dest_ptr, self.left());

                aes_merge(&mut self.lanes[0], &self.buf[0]);
                aes_merge(&mut self.lanes[1], &self.buf[1]);
                aes_merge(&mut self.lanes[2], &self.buf[2]);
                aes_merge(&mut self.lanes[3], &self.buf[3]);
            }

            aes_rotate(&mut r0, &mut self.lanes[0]);
            aes_rotate(&mut r0, &mut self.lanes[1]);
            aes_rotate(&mut r0, &mut self.lanes[2]);
            aes_rotate(&mut r0, &mut self.lanes[3]);

            aes_rotate(&mut r0, &mut self.lanes[0]);
            aes_rotate(&mut r0, &mut self.lanes[1]);
            aes_rotate(&mut r0, &mut self.lanes[2]);
            aes_rotate(&mut r0, &mut self.lanes[3]);

            aes_rotate(&mut r0, &mut self.lanes[0]);
            aes_rotate(&mut r0, &mut self.lanes[1]);
            aes_rotate(&mut r0, &mut self.lanes[2]);
            aes_rotate(&mut r0, &mut self.lanes[3]);

            aes_rotate(&mut r0, &mut self.lanes[0]);
            aes_rotate(&mut r0, &mut self.lanes[1]);
            aes_rotate(&mut r0, &mut self.lanes[2]);
            aes_rotate(&mut r0, &mut self.lanes[3]);

            aes_merge(&mut r0, &empty);
            aes_merge(&mut r0, &empty);
            aes_merge(&mut r0, &empty);
            aes_merge(&mut r0, &empty);
            aes_merge(&mut r0, &empty);
        }

        r0
    }
}

impl Digest for MeowHasher {
    type OutputSize = U64;

    fn new() -> Self {
        Self::with_seed(0)
    }

    fn input<B: AsRef<[u8]>>(&mut self, data: B) {
        let data = data.as_ref();
        let mut src_ptr = data.as_ptr();
        let mut src_left = data.len();
        let mut buf_left = self.left();
        unsafe {
            while src_left >= buf_left {
                ptr::copy_nonoverlapping(src_ptr, self.buf_ptr(), buf_left);

                aes_merge(&mut self.lanes[0], &self.buf[0]);
                aes_merge(&mut self.lanes[1], &self.buf[1]);
                aes_merge(&mut self.lanes[2], &self.buf[2]);
                aes_merge(&mut self.lanes[3], &self.buf[3]);

                src_left -= buf_left;
                src_ptr = src_ptr.add(buf_left);
                buf_left = Self::block_size();
                self.index = 0;
            }
            if src_left > 0 {
                ptr::copy_nonoverlapping(src_ptr, self.buf_ptr(), src_left);
                self.index += src_left;
            }
        }
    }

    fn chain<B: AsRef<[u8]>>(mut self, data: B) -> Self {
        self.input(data);
        self
    }

    fn result(mut self) -> GenericArray<u8, Self::OutputSize> {
        GenericArray::clone_from_slice(self.finalise().as_bytes())
    }

    fn reset(&mut self) {
        *self = Self::with_seed(self.seed);
    }

    fn result_reset(&mut self) -> GenericArray<u8, Self::OutputSize> {
        let result = self.finalise();
        self.reset();
        GenericArray::clone_from_slice(result.as_bytes())
    }

    fn output_size() -> usize {
        Self::OutputSize::USIZE
    }

    /// Compute the hash of a chunk of data directly.
    ///
    /// This is a little faster than using `input` and `result`, because it
    /// doesn't have to keep an intermediate buffer.
    fn digest(data: &[u8]) -> GenericArray<u8, Self::OutputSize> {
        GenericArray::clone_from_slice(meow_hash_1(0, data).as_bytes())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::collection::vec;
    use proptest::num::{u128, u8, usize};

    proptest!{
        #[test]
        fn hash_same_data(seed in u128::ANY, blob in vec(u8::ANY, 0..65536)) {
            let mut hasher = MeowHasher::with_seed(seed);
            hasher.input(&blob);
            let hash1 = hasher.result();
            let hash2 = MeowHasher::digest_with_seed(seed, &blob);
            // Two hashes of the same data are equal
            assert_eq!(hash1, hash2);
        }

        #[test]
        fn hash_different_seeds(seed in u128::ANY, blob in vec(u8::ANY, 0..65536)) {
            let hash1 = MeowHasher::digest_with_seed(seed, &blob);
            let hash2 = MeowHasher::digest_with_seed(seed ^ 1, &blob);
            // Hashes with different seeds are not equal
            assert_ne!(hash1, hash2);
        }

        #[test]
        fn hash_different_data(seed in u128::ANY, mut blob in vec(u8::ANY, 1..65536), modify in usize::ANY) {
            let hash1 = MeowHasher::digest_with_seed(seed, &blob);
            let modify = modify % blob.len();
            blob[modify] ^= 1;
            let hash2 = MeowHasher::digest_with_seed(seed, &blob);
            // A blob with one bit modified hashes differently
            assert_ne!(hash1, hash2);
        }
    }
}
