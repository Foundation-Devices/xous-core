// Xous maintainer's note:
//
// This library is vendored in from the Google OpenSK reference implementation.
// The OpenSK library contains its own implementations of crypto functions.
// The port to Xous attempts to undo that, but where possible leaves a thin
// adapter between the OpenSK custom APIs and the more "standard" Rustcrypto APIs.
// There is always a hazard in adapting crypto APIs and reviewers should take
// note of this. However, by calling out the API differences, it hopefully highlights
// any potential problems in the OpenSK library, rather than papering them over.
//
// Leaving the OpenSK APIs in place also makes it easier to apply upstream
// patches from OpenSK to fix bugs in the code base.

// Original copyright notice preserved below:

// Copyright 2019 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit,
    generic_array::GenericArray, Key, Iv, consts::U16};

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

use super::util::Block16;

pub fn cbc_encrypt(key: &[u8; 32], iv: Block16, blocks: &mut [Block16])
{
    // we get a mut slice of Block16 which is a [u8; 16], and we want a mut slice
    // of GenericArray::<u8, U16>. Unfortunately, I don't think there is any way
    // to do this transformation except either something awful and unsafe, or,
    // making a heap allocated copy into and out of the structures. Since the
    // data handled by the authenticator is small, and we value correctness,
    // we are going to go the inefficient-but-safe route.
    let mut ga = vec![];
    for block in blocks.iter() {
        ga.push(GenericArray::<u8, U16>::clone_from_slice(block));
    }
    Aes256CbcEnc::new(Key::<Aes256CbcEnc>::from_slice(key), Iv::<Aes256CbcEnc>::from_slice(&iv))
        .encrypt_blocks_mut(&mut ga);
    for (src, dst) in ga.iter().zip(blocks.iter_mut()) {
        dst.copy_from_slice(src.as_slice());
    }
}

pub fn cbc_decrypt(key: &[u8; 32], iv: Block16, blocks: &mut [Block16])
{
    let mut ga = vec![];
    for block in blocks.iter() {
        ga.push(GenericArray::<u8, U16>::clone_from_slice(block));
    }
    Aes256CbcDec::new(Key::<Aes256CbcDec>::from_slice(key), Iv::<Aes256CbcDec>::from_slice(&iv))
        .decrypt_blocks_mut(&mut ga);
    for (src, dst) in ga.iter().zip(blocks.iter_mut()) {
        dst.copy_from_slice(src.as_slice());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::xor_block_16;
    use aes::Aes256Soft as Aes256;
    use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray, BlockDecrypt};

    #[test]
    fn test_cbc_encrypt_decrypt() {
        // Test that cbc_decrypt is the inverse of cbc_encrypt for a bunch of block values.
        let enc_key = &[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let dec_key = enc_key;

        for len in 0..16 {
            let mut blocks: Vec<Block16> = vec![Default::default(); len];
            for i in 0..len {
                for j in 0..16 {
                    blocks[i][j] = ((len + i) * 16 + j) as u8;
                }
            }
            let iv = [
                0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
                0x2e, 0x2f,
            ];
            let expected = blocks.clone();

            cbc_encrypt(&enc_key, iv, &mut blocks);
            cbc_decrypt(&dec_key, iv, &mut blocks);
            assert_eq!(blocks, expected);
        }
    }

    #[test]
    fn test_cbc_encrypt_1block_zero_iv() {
        let key = &[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let mut blocks = [[
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ]];
        let iv = [0; 16];
        cbc_encrypt(&key, iv, &mut blocks);

        let mut expected = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ];
        let cipher = Aes256::new(GenericArray::from_slice(key));
        cipher.encrypt_block(GenericArray::from_mut_slice(&mut expected));
        // key.encrypt_block(&mut expected);

        assert_eq!(blocks, [expected]);
    }

    #[test]
    fn test_cbc_decrypt_1block_zero_iv() {
        let key = &[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let mut blocks = [[
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ]];
        let iv = [0; 16];
        cbc_decrypt(&key, iv, &mut blocks);

        let mut expected = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ];
        let cipher = Aes256::new(GenericArray::from_slice(key));
        cipher.decrypt_block(GenericArray::from_mut_slice(&mut expected));
        // key.decrypt_block(&mut expected);

        assert_eq!(blocks, [expected]);
    }

    #[test]
    fn test_cbc_encrypt_1block() {
        let key = &[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let mut blocks = [[
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ]];
        let iv = [
            0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d,
            0x3e, 0x3f,
        ];
        cbc_encrypt(&key, iv, &mut blocks);

        let mut expected = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ];
        xor_block_16(&mut expected, &iv);
        let cipher = Aes256::new(GenericArray::from_slice(key));
        cipher.encrypt_block(GenericArray::from_mut_slice(&mut expected));
        // key.encrypt_block(&mut expected);

        assert_eq!(blocks, [expected]);
    }

    #[test]
    fn test_cbc_decrypt_1block() {
        let key = &[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let mut blocks = [[
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ]];
        let iv = [
            0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d,
            0x3e, 0x3f,
        ];
        cbc_decrypt(&key, iv, &mut blocks);

        let mut expected = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ];
        let cipher = Aes256::new(GenericArray::from_slice(key));
        cipher.decrypt_block(GenericArray::from_mut_slice(&mut expected));
        // key.decrypt_block(&mut expected);
        xor_block_16(&mut expected, &iv);

        assert_eq!(blocks, [expected]);
    }

    #[test]
    fn test_cbc_encrypt_2blocks() {
        let key = &[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let mut blocks = [
            [
                0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
                0x2e, 0x2f,
            ],
            [
                0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
                0x4e, 0x4f,
            ],
        ];
        let iv = [
            0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d,
            0x3e, 0x3f,
        ];
        cbc_encrypt(&key, iv, &mut blocks);

        let mut expected0 = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ];
        let mut expected1 = [
            0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
            0x4e, 0x4f,
        ];
        xor_block_16(&mut expected0, &iv);
        let cipher = Aes256::new(GenericArray::from_slice(key));
        cipher.encrypt_block(GenericArray::from_mut_slice(&mut expected0));
        // key.encrypt_block(&mut expected0);
        xor_block_16(&mut expected1, &expected0);
        let cipher = Aes256::new(GenericArray::from_slice(key));
        cipher.encrypt_block(GenericArray::from_mut_slice(&mut expected1));
        // key.encrypt_block(&mut expected1);

        assert_eq!(blocks, [expected0, expected1]);
    }

    #[test]
    fn test_cbc_decrypt_2blocks() {
        let key = &[
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ];

        let mut blocks = [
            [
                0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
                0x2e, 0x2f,
            ],
            [
                0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
                0x4e, 0x4f,
            ],
        ];
        let iv = [
            0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d,
            0x3e, 0x3f,
        ];
        cbc_decrypt(&key, iv, &mut blocks);

        let mut expected0 = [
            0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f,
        ];
        let mut expected1 = [
            0x40, 0x41, 0x42, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d,
            0x4e, 0x4f,
        ];
        let cipher = Aes256::new(GenericArray::from_slice(key));
        cipher.decrypt_block(GenericArray::from_mut_slice(&mut expected1));
        // key.decrypt_block(&mut expected1);
        xor_block_16(&mut expected1, &expected0);
        let cipher = Aes256::new(GenericArray::from_slice(key));
        cipher.decrypt_block(GenericArray::from_mut_slice(&mut expected0));
        // key.decrypt_block(&mut expected0);
        xor_block_16(&mut expected0, &iv);

        assert_eq!(blocks, [expected0, expected1]);
    }
}
