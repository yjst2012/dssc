use super::varint::{put_uvarint, uvarint};
use super::{Compressor, VecCache};

const CHUNK_SIZE: usize = 4;

pub struct ChunkedCompressor {}

impl Compressor for ChunkedCompressor {
    fn compress(&self, needle: &[u8], out_buf: &mut Vec<u8>, cache: &VecCache) -> usize {
        if cache.len() == 0 {
            out_buf.push(0);
            Block {
                block_type: BlockType::Original,
                offset: 0,
                needle_off: 0,
                len: needle.len(),
            }.encode(needle, out_buf);
            return 0;
        }
        let matches = chunk_match(needle, &cache);
        eprintln!("matches {:?}", matches);
        let max: usize = matches
            .iter()
            .map(|m| m.iter().filter(|&&o| o != 0).count())
            .max()
            .expect("haystacks are empty");
        let mut max_block = (0, 0, None);
        for (hi, result) in matches.iter().enumerate() {
            if result.iter().filter(|&&o| o != 0).count() != max {
                continue;
            }
            let blocks = expand_blocks(needle, &cache[hi].data, result);
            let score = blocks
                .iter()
                .filter(|b| b.block_type == BlockType::Delta)
                .map(|b| b.len)
                .sum();
            if max_block.0 <= score {
                max_block = (score, hi, Some(blocks))
            }
        }
        eprintln!("{:?}", max_block.2);
        out_buf.push(max_block.1 as u8);
        for block in max_block.2.expect("No candidate was found") {
            block.encode(needle, out_buf);
        }
        eprintln!("{:?} needle", needle);
        eprintln!("{:?} output", out_buf);
        max_block.1
    }

    fn decompress(&self, buf: &[u8], out_buf: &mut Vec<u8>, haystacks: &VecCache) -> usize {
        let hi = buf[0] as usize;
        let mut bi = 1;
        if haystacks.len() == 0 {
            bi += 1;
            let (len, len_len) = uvarint(&buf[bi..]);
            if len_len <= 0 {
                panic!("Something is wrong with length varint");
            }
            bi += len_len as usize;
            out_buf.extend_from_slice(&buf[bi..bi + len as usize]);
            return 0;
        }
        while bi < buf.len() {
            if buf[bi] == 0 {
                //original
                bi += 1;
                let (len, len_len) = uvarint(&buf[bi..]);
                if len_len <= 0 {
                    panic!("Something is wrong with length varint");
                }
                bi += len_len as usize;
                out_buf.extend_from_slice(&buf[bi..bi + len as usize]);
                bi += len as usize;
            } else {
                let (mut offset, offset_len) = uvarint(&buf[bi..]);
                if offset_len <= 0 {
                    panic!("Something is wrong with offset varint");
                }
                offset -= 1;
                bi += offset_len as usize;
                let (len, len_len) = uvarint(&buf[bi..]);
                if len_len <= 0 {
                    panic!("Something is wrong with length varint");
                }
                bi += len_len as usize;
                out_buf.extend_from_slice(
                    &haystacks[hi].data[offset as usize..offset as usize + len as usize],
                );
            }
        }
        hi
    }
}

// for each haystack returns a list of indexes where each chunk of needle was found, 0 means not found
fn chunk_match(needle: &[u8], haystacks: &VecCache) -> Vec<Vec<usize>> {
    let mut results = Vec::new();
    for haystack in haystacks {
        results.push(
            needle
                .chunks(CHUNK_SIZE)
                .map(|chunk| {
                    for hi in 0..haystack.data.len() - (CHUNK_SIZE - 1) {
                        if &haystack.data[hi..hi + CHUNK_SIZE] == chunk {
                            return hi + 1;
                        }
                    }
                    return 0;
                })
                .collect(),
        )
    }
    return results;
}

#[derive(Debug, PartialEq)]
enum BlockType {
    Delta,
    Original,
}

#[derive(Debug)]
struct Block {
    block_type: BlockType,
    needle_off: usize,
    offset: usize,
    len: usize,
}

impl Block {
    fn fit(&mut self, needle: &[u8], haystack: &Vec<u8>) {
        if self.block_type != BlockType::Delta {
            return;
        }
        let mut bi = 1;
        let mut od = 0;
        let mut fi = self.len;
        while self.needle_off > bi && self.offset > bi &&
            needle[self.needle_off - bi] == haystack[self.offset - bi]
        {
            od += 1;
            self.len += 1;
            bi += 1;
        }
        while (self.needle_off + fi) < needle.len() && (self.offset + fi) < haystack.len() &&
            needle[self.needle_off + fi] == haystack[self.offset + fi]
        {
            self.len += 1;
            fi += 1;
        }
        self.offset -= od;
        self.needle_off -= od;
    }

    fn encode(&self, needle: &[u8], buf: &mut Vec<u8>) {
        let mut varint_buf = [0; 10];
        match self.block_type {
            BlockType::Delta => {
                let offset_len = put_uvarint(&mut varint_buf, (self.offset + 1) as u64);
                buf.extend_from_slice(&varint_buf[0..offset_len]);
                let len_len = put_uvarint(&mut varint_buf, (self.len) as u64);
                buf.extend_from_slice(&varint_buf[0..len_len]);
            }
            BlockType::Original => {
                buf.push(0 as u8);
                let len_len = put_uvarint(&mut varint_buf, (self.len) as u64);
                buf.extend_from_slice(&varint_buf[0..len_len]);
                buf.extend_from_slice(&needle[self.needle_off..self.needle_off + self.len])
            }
        }
    }
}


fn expand_blocks(needle: &[u8], haystack: &Vec<u8>, result: &Vec<usize>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut ri = 0;
    while ri < result.len() {
        if result[ri] != 0 {
            let bi = ri;
            let (offset, mut len) = (result[ri] - 1, CHUNK_SIZE);
            ri += 1;
            while ri < result.len() && result[ri] == result[ri - 1] + CHUNK_SIZE {
                len += CHUNK_SIZE;
                ri += 1;
            }
            let mut block = Block {
                block_type: BlockType::Delta,
                needle_off: bi * CHUNK_SIZE,
                offset: offset,
                len: len,
            };
            block.fit(needle, haystack);
            blocks.push(block);
        } else {
            let block = if let Some(last_block) = blocks.last() {
                Block {
                    block_type: BlockType::Original,
                    needle_off: last_block.needle_off + last_block.len,
                    offset: 0,
                    len: 0,
                }
            } else {
                Block {
                    block_type: BlockType::Original,
                    needle_off: 0,
                    offset: 0,
                    len: 0,
                }
            };
            blocks.push(block);
            while ri < result.len() && result[ri] == 0 {
                ri += 1;
            }
        }
    }
    for i in 0..blocks.len() - 1 {
        if blocks[i].block_type == BlockType::Original {
            blocks[i].len = blocks[i + 1].needle_off - blocks[i].needle_off;
        }
    }
    //println!("blocks {:?}", blocks);
    if let Some(ref mut last_block) = blocks.last_mut() {
        if last_block.block_type == BlockType::Original {
            last_block.len = needle.len() - last_block.needle_off;
        }
    }
    blocks.retain(|b| b.len != 0);
    blocks
}


/*
#[test]
pub fn chunk_test() {
    let haystacks = vec![
        "Hello Denis Worlds".as_bytes().to_vec(),
        "Test Worlds".as_bytes().to_vec(),
        "Test Bananas".as_bytes().to_vec(),
    ];
    let compressed = chunk_compressor("Hello Test Worlds".as_bytes(), &haystacks);
    println!("{:?}", chunk_decompressor(&compressed, &haystacks));
}
*/