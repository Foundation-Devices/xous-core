use graphics_server::api::shapes;
use graphics_server::api::{Point, Rectangle, Tile};
use graphics_server::PixelColor;

use core::cmp::min;
use dither::prelude::{Dither, Img, RGB};
use std::convert::TryInto;
use std::ops::Deref;

/*
 * The basic idea is to define a Bitmap as a variable mosaic of Sized Tiles.
 *
 * Each Tile contains an Array of u32 Words, a bounding Rectangle, and a Word width.
 * This is arranged to come in just under 4096 bytes, allowing for the rkyv overhead.
 * Each line of bits across the Tile is packed into an Integer number of u32 Words.
 *
 * The Bitmap contains a bounding Rectangle and a Vec of Tiles. The current implmentation
 * has a very simple tiling strategy - a single vertical strip of full-width tiles.
 * All tiles are the same width and same maximum height - except the last Tile which may
 * have some unused Words at the end of the Array. More space efficient tiling strategies
 * are possible - but likely with a processing and code complexity overhead.
 *
 * author: nworbnhoj
 */

#[derive(Debug, Clone)]
pub struct Bitmap {
    pub bound: Rectangle,
    tile_size: Point,
    mosaic: Vec<Tile>,
}

impl Bitmap {
    pub fn new(size: Point) -> Self {
        let bound = Rectangle::new(Point::new(0, 0), size);
        log::trace!("new Bitmap {:?}", bound);

        let (tile_size, tile_width_words) = Bitmap::tile_spec(size);
        let tile_height = tile_size.y as usize;
        let bm_height = (size.y + 1) as usize;
        let tile_count = match bm_height % tile_height {
            0 => bm_height / tile_height,
            _ => bm_height / tile_height + 1,
        };

        let mut mosaic: Vec<Tile> = Vec::new();
        for y in 0..tile_count {
            let tl = Point::new(0, (y * tile_height) as i16);
            let mut br = Point::new(tile_size.x - 1, ((y + 1) * tile_height - 1) as i16);
            if br.y > size.y {
                br.y = size.y;
            }
            let tile = Tile::new(Rectangle::new(tl, br), tile_width_words as u16);
            mosaic.push(tile);
        }
        Self {
            bound,
            tile_size,
            mosaic,
        }
    }

    fn tile_spec(bm_size: Point) -> (Point, i16) {
        let bm_width_bits = 1 + bm_size.x as usize;
        let mut tile_width_bits = bm_width_bits;
        let tile_width_words = if bm_width_bits > shapes::BITS_PER_TILE {
            log::warn!("Bitmap max width exceeded");
            tile_width_bits = shapes::WORDS_PER_TILE * shapes::BITS_PER_WORD;
            shapes::WORDS_PER_TILE
        } else {
            match bm_width_bits % shapes::BITS_PER_WORD {
                0 => bm_width_bits / shapes::BITS_PER_WORD,
                _ => bm_width_bits / shapes::BITS_PER_WORD + 1,
            }
        };
        let tile_height_bits = shapes::WORDS_PER_TILE / tile_width_words;
        let tile_size = Point::new(tile_width_bits as i16, tile_height_bits as i16);
        (tile_size, tile_width_words as i16)
    }

    fn size(&self) -> (usize, usize) {
        (self.bound.br.x as usize, self.bound.br.y as usize)
    }

    fn get_tile_index(&self, point: Point) -> usize {
        if self.bound.intersects_point(point) {
            let x = point.x as usize;
            let y = point.y as usize;
            let tile_width = self.tile_size.x as usize;
            let tile_height = self.tile_size.y as usize;
            let tile_size_bits = tile_width * tile_height;
            (x + y * tile_width) / tile_size_bits
        } else {
            log::warn!("Out of bounds {:?}", point);
            0
        }
    }

    pub fn get_tile(&self, point: Point) -> Tile {
        let tile = self.get_tile_index(point);
        self.mosaic.as_slice()[tile]
    }

    fn get_mut_tile(&mut self, point: Point) -> &mut Tile {
        let tile = self.get_tile_index(point);
        &mut self.mosaic.as_mut_slice()[tile]
    }

    pub fn get_line(&self, point: Point) -> Vec<shapes::Word> {
        self.get_tile(point).get_line(point)
    }

    fn get_word(&self, point: Point) -> shapes::Word {
        self.get_tile(point).get_word(point)
    }

    pub fn get_pixel(&self, point: Point) -> PixelColor {
        self.get_tile(point).get_pixel(point)
    }

    pub fn set_pixel(&mut self, point: Point, color: PixelColor) {
        self.get_mut_tile(point).set_pixel(point, color)
    }

    pub fn translate(&mut self, offset: Point) {
        for tile in self.mosaic.as_mut_slice() {
            tile.translate(offset);
        }
    }
}

impl Deref for Bitmap {
    type Target = Vec<Tile>;

    fn deref(&self) -> &Self::Target {
        &self.mosaic
    }
}

impl From<Img<RGB<u8>>> for Bitmap {
    fn from(img: Img<RGB<u8>>) -> Self {
        let img = img.convert_with(|rgb| rgb.convert_with(<f64 as From<u8>>::from));
        let bit_depth = 1;
        let quantize = dither::create_quantize_n_bits_func(bit_depth).unwrap();
        let bw_img = img.convert_with(|rgb| rgb.to_chroma_corrected_black_and_white());
        let ditherer = dither::ditherer::FLOYD_STEINBERG;
        let output_img = ditherer
            .dither(bw_img, quantize)
            .convert_with(RGB::from_chroma_corrected_black_and_white);

        let bm_width: usize = output_img.width().try_into().unwrap();
        let img_vec = output_img.into_vec();

        /*
        let bw_vec = Vec::<PixelColor>::new();
        for pixel in img_vec {
            let color = match pixel.to_hex() {
                0 => PixelColor::Light,
                _ => PixelColor::Dark,
            };
            bw_vec.push(color);
        }
        */

        let bm_height = img_vec.len() / bm_width;
        let bm_bottom = (bm_height - 1) as i16;
        let bm_right = (bm_width - 1) as i16;
        let bm_br = Point::new(bm_right, bm_bottom);
        let bound = Rectangle::new(Point::new(0, 0), bm_br);

        let (tile_size, tile_width_words) = Bitmap::tile_spec(bm_br);
        let tile_height = tile_size.y as usize;
        let tile_count = match bm_height % tile_height {
            0 => bm_height / tile_height,
            _ => bm_height / tile_height + 1,
        };
        let mut mosaic: Vec<Tile> = Vec::new();

        let mut img_vec_index = 0;
        for t in 0..tile_count {
            let t_top = (t * tile_height) as i16;
            let t_left = 0;
            let t_bottom = min(bm_bottom, ((t + 1) * tile_height - 1) as i16);
            let t_right = tile_size.x - 1;
            let t_tl = Point::new(t_left, t_top);
            let t_br = Point::new(t_right, t_bottom);
            let t_bound = Rectangle::new(t_tl, t_br);
            let mut tile = Tile::new(t_bound, tile_width_words as u16);
            for y in t_top..=t_bottom {
                // TODO performance gain here by utilizing Tile.set_line()
                for x in t_left..=t_right {
                    let pixel = img_vec[img_vec_index];
                    let color = match pixel.to_hex() {
                        0 => PixelColor::Light,
                        _ => PixelColor::Dark,
                    };
                    tile.set_pixel(Point::new(x, y), color);
                    img_vec_index += 1;
                }
            }
            mosaic.push(tile);
        }
        Self {
            bound,
            tile_size,
            mosaic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]

    fn bitmap_test() {
        let x_size = 100;
        let y_size = 10;
        let bm = Bitmap::new(Point::new(x_size, y_size));
        assert_equal!(bm.size.x, x_size);
        assert_equal!(bm.size.y, y_size);
        assert_equal!(bm.get(5, 5), PixelColor::Light);
        bm.set(5, 5, PixelColor::Dark);
        assert_equal!(bm.get(5, 5), PixelColor::Dark);
    }
}
