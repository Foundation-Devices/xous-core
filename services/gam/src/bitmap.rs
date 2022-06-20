use core::cmp::{max, min};
use graphics_server::api::*;
use graphics_server::PixelColor;
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

#[derive(Debug)]
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

    pub fn new_resize(image: &Img, width: usize) -> Self {
        let (img_width, _, _) = image.size();
        let pixels = image.iter().shrink(img_width, width).collect();
        let image = Img::new(pixels, width, PixelType::U8);
        let (img_width, img_height, _) = image.size();

        let bm_bottom = img_height - 1;
        let bm_right = img_width - 1;
        let bm_br = Point::new(bm_right as i16, bm_bottom as i16);
        let bound = Rectangle::new(Point::new(0, 0), bm_br);

        let (tile_size, tile_width_words) = Bitmap::tile_spec(bm_br);
        let tile_height: usize = tile_size.y.try_into().unwrap();
        let tile_count = match img_height % tile_height {
            0 => img_height / tile_height,
            _ => img_height / tile_height + 1,
        };
        let mut mosaic: Vec<Tile> = Vec::new();

        let words = Dither::new(BURKES.to_vec()).dither(&image);
        let mut wd_index = 0;
        let bits_per_word: i16 = BITS_PER_WORD.try_into().unwrap();
        for t in 0..tile_count {
            let t_top = t * tile_height;
            let t_left = 0;
            let t_bottom = min(bm_bottom, (t + 1) * tile_height - 1);
            let t_right = tile_size.x - 1;
            let t_tl = Point::new(t_left, t_top.try_into().unwrap());
            let t_br = Point::new(t_right, t_bottom.try_into().unwrap());
            let t_bound = Rectangle::new(t_tl, t_br);
            let mut tile = Tile::new(t_bound, tile_width_words.try_into().unwrap());
            for y in t_top..=t_bottom {
                let mut x = t_left;
                while x <= t_right {
                    let word = words[wd_index];
                    wd_index += 1;
                    let anchor = Point::new(x.try_into().unwrap(), y.try_into().unwrap());
                    tile.set_word(anchor, word.try_into().unwrap());
                    x += bits_per_word;
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

    fn tile_spec(bm_size: Point) -> (Point, i16) {
        let bm_width_bits = 1 + bm_size.x as usize;
        let mut tile_width_bits = bm_width_bits;
        let tile_width_words = if bm_width_bits > BITS_PER_TILE {
            log::warn!("Bitmap max width exceeded");
            tile_width_bits = WORDS_PER_TILE * BITS_PER_WORD;
            WORDS_PER_TILE
        } else {
            match bm_width_bits % BITS_PER_WORD {
                0 => bm_width_bits / BITS_PER_WORD,
                _ => bm_width_bits / BITS_PER_WORD + 1,
            }
        };
        let tile_height_bits = WORDS_PER_TILE / tile_width_words;
        let tile_size = Point::new(tile_width_bits as i16, tile_height_bits as i16);
        (tile_size, tile_width_words as i16)
    }

    #[allow(dead_code)]
    fn area(&self) -> u32 {
        let (x, y) = self.size();
        (x * y) as u32
    }

    pub fn size(&self) -> (usize, usize) {
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

    fn hull(mosaic: &Vec<Tile>) -> Rectangle {
        let mut hull_tl = Point::new(i16::MAX, i16::MAX);
        let mut hull_br = Point::new(i16::MIN, i16::MIN);
        let mut tile_area = 0;
        for (_i, tile) in mosaic.iter().enumerate() {
            let tile_bound = tile.bound();
            hull_tl.x = min(hull_tl.x, tile_bound.tl.x);
            hull_tl.y = min(hull_tl.y, tile_bound.tl.y);
            hull_br.x = max(hull_br.x, tile_bound.br.x);
            hull_br.y = max(hull_br.y, tile_bound.br.y);
            tile_area +=
                (1 + tile_bound.br.x - tile_bound.tl.x) * (1 + tile_bound.br.y - tile_bound.tl.y);
        }
        let hull_area = (1 + hull_br.x - hull_tl.x) * (1 + hull_br.y - hull_tl.y);
        if tile_area < hull_area {
            log::warn!("Bitmap Tile gaps");
        } else if tile_area > hull_area {
            log::warn!("Bitmap Tile overlap");
        }
        Rectangle::new(hull_tl, hull_br)
    }

    pub fn get_tile(&self, point: Point) -> Tile {
        let tile = self.get_tile_index(point);
        self.mosaic.as_slice()[tile]
    }

    fn get_mut_tile(&mut self, point: Point) -> &mut Tile {
        let tile = self.get_tile_index(point);
        &mut self.mosaic.as_mut_slice()[tile]
    }

    pub fn get_line(&self, point: Point) -> Vec<Word> {
        self.get_tile(point).get_line(point)
    }

    fn get_word(&self, point: Point) -> Word {
        self.get_tile(point).get_word(point)
    }

    fn set_word(&mut self, point: Point, word: Word) {
        self.get_mut_tile(point).set_word(point, word);
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

    pub fn rotate90(&mut self) -> Self {
        let bits_per_word: i16 = BITS_PER_WORD.try_into().unwrap();
        let (size_x, size_y) = self.size();
        let size_x: i16 = size_x.try_into().unwrap();
        let size_y: i16 = size_y.try_into().unwrap();
        let mut r90 = Bitmap::new(Point::new(size_y, size_x));
        let (_, r90_size_y) = r90.size();

        let mut x: i16 = 0;
        let mut r90_y = 0;
        let mut block: [Word; BITS_PER_WORD] = [0; BITS_PER_WORD];
        while x < size_x {
            let mut y = size_y - 1;
            let mut r90_x = 0;
            // extract a square block of bits - ie 32 x u32 words
            // beginning from bottom-left, and progressing up in strips, from left to right
            while y >= 0 {
                let mut b = 0;
                while b < block.len() {
                    block[b] = if y >= 0 {
                        self.get_word(Point::new(x, y))
                    } else {
                        0
                    };
                    y -= 1;
                    b += 1;
                }
                // rotate the block and write to r90
                // beginning from top-left, and progressing right in strips, from top to bottom
                for w in 0..bits_per_word {
                    if r90_y + w >= r90_size_y.try_into().unwrap() {
                        continue;
                    }
                    let mut word: Word = 0;
                    for b in 0..block.len() {
                        word = word | (((block[b] >> w) & 1) << b);
                    }
                    r90.set_word(Point::new(r90_x, r90_y + w), word);
                }
                r90_x = r90_x + bits_per_word;
            }
            x = x + bits_per_word;
            r90_y = r90_y + bits_per_word;
        }
        r90
    }
}

impl Deref for Bitmap {
    type Target = Vec<Tile>;

    fn deref(&self) -> &Self::Target {
        &self.mosaic
    }
}

impl From<[Option<Tile>; 6]> for Bitmap {
    fn from(tiles: [Option<Tile>; 6]) -> Self {
        let mut mosaic: Vec<Tile> = Vec::new();
        let mut tile_size = Point::new(0, 0);
        for t in 0..tiles.len() {
            if tiles[t].is_some() {
                let tile = tiles[t].unwrap();
                mosaic.push(tile);
                if tile_size.x == 0 {
                    tile_size = tile.size();
                }
            }
        }

        Self {
            bound: Self::hull(&mosaic),
            tile_size: tile_size,
            mosaic: mosaic,
        }
    }
}

impl From<&Img> for Bitmap {
    fn from(image: &Img) -> Self {
        let (img_width, _, _) = image.size();
        Bitmap::new_resize(image, img_width)
    }
}

// **********************************************************************

pub enum PixelType {
    U8,
    U8x3,
    U8x4,
}

pub struct GreyScale<I> {
    iter: I,
    px_type: PixelType,
}

impl<'a, I: Iterator<Item = &'a u8>> GreyScale<I> {
    fn new(iter: I, px_type: PixelType) -> GreyScale<I> {
        Self { iter, px_type }
    }
}

impl<'a, I: Iterator<Item = &'a u8>> Iterator for GreyScale<I> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        // chromatic coversion from RGB to Greyscale
        const R: u32 = 2126;
        const G: u32 = 7152;
        const B: u32 = 722;
        const BLACK: u32 = R + G + B;
        match self.px_type {
            PixelType::U8 => match self.iter.next() {
                Some(gr) => Some(*gr),
                None => None,
            },
            PixelType::U8x3 => {
                let r = self.iter.next();
                let g = self.iter.next();
                let b = self.iter.next();
                if r.is_some() && g.is_some() && b.is_some() {
                    let grey_r = R * *r.unwrap() as u32;
                    let grey_g = G * *g.unwrap() as u32;
                    let grey_b = B * *b.unwrap() as u32;
                    let grey: u8 = ((grey_r + grey_g + grey_b) / BLACK).try_into().unwrap();
                    Some(grey)
                } else {
                    None
                }
            }
            _ => {
                log::warn!("unsupported PixelType");
                None
            }
        }
    }
}

pub trait GreyScaleIterator<'a>: Iterator<Item = &'a u8> + Sized {
    /// converts pixels of PixelType to u8 greyscale
    fn to_grey(self, px_type: PixelType) -> GreyScale<Self> {
        GreyScale::new(self, px_type)
    }
}

impl<'a, I: Iterator<Item = &'a u8>> GreyScaleIterator<'a> for I {}

// **********************************************************************

pub struct Shrink<I> {
    /// iterator over inbound pixels
    iter: I,
    /// width of the outbound image
    out_width: usize,
    /// the scale factor between inbound and outbound images (ie in_width/out_width)
    scale: f32,
    /// a pre-tabulated list of the trailing edge of each inbound strip of pixels
    in_x_cap: Vec<u16>,
    /// the current y coord of the inbound image
    in_y: usize,
    /// the current x coord of the outbound image
    out_x: usize,
    /// the current y coord of the outbound image    
    out_y: usize,
    /// a buffer the width of the outbound image to stove horizontal averages
    buf: Vec<u16>,
    /// the width of the current stri in the inbound image
    y_div: u16,
    /// the x coord of the final pixel in the inbound image
    out_x_last: usize,
}

impl<'a, I: Iterator<Item = &'a u8>> Shrink<I> {
    fn new(iter: I, in_width: usize, out_width: usize) -> Shrink<I> {
        let scale = in_width as f32 / out_width as f32;
        // set up a buffer to average the surrounding pixels
        let buf: Vec<u16> = if scale <= 1.0 {
            Vec::new()
        } else {
            vec![0u16; out_width]
        };

        // Pretabulate horizontal pixel positions
        let mut in_x_cap: Vec<u16> = Vec::with_capacity(out_width);
        let max_width: u16 = (in_width - 1).try_into().unwrap();
        for out_x in 1..=out_width {
            let in_x: u16 = (scale * out_x as f32) as u16;
            in_x_cap.push((in_x).min(max_width));
        }
        let out_x_last = out_width;

        Self {
            iter,
            out_width,
            scale,
            in_x_cap,
            in_y: 0,
            out_x: 0,
            out_y: 1,
            buf,
            y_div: 0,
            out_x_last,
        }
    }

    #[allow(dead_code)]
    fn next_xy(&self) -> (usize, usize) {
        (self.out_x, self.out_y)
    }
}
/// Adaptor Iterator to shrink an image dimensions from in_width to out_width
impl<'a, I: Iterator<Item = &'a u8>> Iterator for Shrink<I> {
    type Item = u8;

    /// Shrinks an image from in_width to out_width
    /// The algorithm divides the inbound image into vertical and horivontal strips,
    /// correspoding to the columns and rows of the outbound image. Each outbound
    /// pixel is the average of the pixels contained within each intersaction of
    /// vertical and horizontal strips. For example, when in_width = 3 x out_width
    /// each outbound pixel will be the average of 9 pixels in a 3x3 inbound block.
    /// Note that with a non-integer scale the strips will be of variable width.
    fn next(&mut self) -> Option<Self::Item> {
        // if there is no reduction in image size then simple return image as-is
        if self.scale <= 1.0 {
            return match self.iter.next() {
                Some(pixel) => Some(*pixel),
                None => None,
            };
        }
        // processed the last inbound pixel
        if self.out_x > self.out_x_last {
            return None;
        }
        // take the average of pixels in the horizontal, and then vertical.
        let in_y_cap = (self.scale * self.out_y as f32) as usize;
        while self.in_y < in_y_cap {
            let mut in_x = 0;
            for (out_x, in_x_cap) in self.in_x_cap.iter().enumerate() {
                let mut x_total: u16 = 0;
                let mut x_div: u16 = 0;
                while in_x <= *in_x_cap {
                    x_total += match self.iter.next() {
                        Some(pixel) => *pixel as u16,
                        None => {
                            self.out_x_last = out_x - 1;
                            0
                        }
                    };
                    in_x += 1;
                    x_div += 1;
                }
                self.buf[out_x] += x_total / x_div;
            }
            self.in_y += 1;
            self.y_div += 1;
        }
        // calculate the average of the sum of pixels in the buffer, and reset buffer
        let pixel: u8 = (self.buf[self.out_x] / self.y_div).try_into().unwrap();
        self.buf[self.out_x] = 0;
        // prepare for the next pixel in the row or column
        self.out_x += 1;
        if self.out_x >= self.out_width {
            self.out_x = 0;
            self.out_y += 1;
            self.y_div = 0;
        }
        Some(pixel)
    }
}

pub trait ShrinkIterator<'a>: Iterator<Item = &'a u8> + Sized {
    fn shrink(self, in_width: usize, out_width: usize) -> Shrink<Self> {
        Shrink::new(self, in_width, out_width)
    }
}

impl<'a, I: Iterator<Item = &'a u8>> ShrinkIterator<'a> for I {}

// **********************************************************************

/*
 * Image as a minimal flat buffer of grey u8 pixels; accessible by (x, y)
 *
 * author: nworbnhoj
 */

#[derive(Debug)]
pub struct Img {
    pub pixels: Vec<u8>,
    pub width: usize,
}

impl Img {
    pub fn new(buf: Vec<u8>, width: usize, px_type: PixelType) -> Self {
        let px_len = match px_type {
            PixelType::U8 => buf.len(),
            PixelType::U8x3 => buf.len() / 3,
            _ => 0,
        };
        let mut pixels: Vec<u8> = Vec::with_capacity(px_len);

        for pixel in buf.iter().to_grey(px_type) {
            pixels.push(pixel);
        }
        Self { pixels, width }
    }
    pub fn get(&self, x: usize, y: usize) -> Option<&u8> {
        let i: usize = (y * self.width) + x;
        self.pixels.get(i)
    }
    pub fn size(&self) -> (usize, usize, usize) {
        let width: usize = self.width.try_into().unwrap();
        let length: usize = self.pixels.len().try_into().unwrap();
        let height: usize = length / width;
        (width, height, length)
    }
    pub fn as_slice(&self) -> &[u8] {
        self.pixels.as_slice()
    }
}

impl Deref for Img {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.pixels
    }
}

/*
 * Dithering involves aplying a threshold to each pixel to round down to Black
 * or up to White. The residual error from this blunt instrument is diffused amongst
 * the surrounding pixels. So the luminosity lost by forcing a pixel down to Black,
 * results in the surrounding pixels incrementally more likely to round up to White.
 * Pixels are processed from left to right and then top to bottom. The residual
 * error from each Black/White determination is carried-forward to pixels to the
 * right and below as per the diffusion scheme.
 * https://tannerhelland.com/2012/12/28/dithering-eleven-algorithms-source-code.html
 *
 * author: nworbnhoj
 */

/// Burkes dithering diffusion scheme was chosen for its modest resource
/// requirements with impressive quality outcome.
/// Burkes dithering. Div=32.
/// - ` .  .  x  8  4`
/// - ` 2  4  8  4  2`
const BURKES: [(isize, isize, i16); 7] = [
    // (dx, dy, mul)
    (1, 0, 8),
    (2, 0, 4),
    //
    (-2, 1, 2),
    (-1, 1, 4),
    (0, 1, 8),
    (1, 1, 4),
    (2, 1, 2),
];

struct Dither {
    // the width of the image to be dithered
    width: usize,
    // the error diffusion scheme (dx, dy, multiplier)
    diffusion: Vec<(isize, isize, i16)>,
    // the sum of the multipliers in the diffusion
    denominator: i16,
    // a circular array of errors representing dy rows of the image,
    err: Vec<i16>,
    // the position in err representing the carry forward error for the current pixel
    origin: usize,
}

impl Dither {
    const THRESHOLD: i16 = u8::MAX as i16 / 2;
    pub fn new(diffusion: Vec<(isize, isize, i16)>) -> Self {
        let mut denominator: i16 = 0;
        for (_, _, mul) in &diffusion {
            denominator += mul;
        }
        Self {
            width: 0,
            diffusion,
            denominator,
            err: Vec::<i16>::new(),
            origin: 0,
        }
    }
    fn provision(&mut self, width: usize) {
        self.width = width;
        let (mut max_dx, mut max_dy) = (0, 0);
        for (dx, dy, _) in &self.diffusion {
            max_dx = max(*dx, max_dx);
            max_dy = max(*dy, max_dy);
        }
        let length: usize = width * max_dy as usize + max_dx as usize + 1;
        self.err = vec![0i16; length];
    }
    fn index(&self, dx: isize, dy: isize) -> usize {
        let width: isize = self.width.try_into().unwrap();
        let offset: usize = (width * dy + dx).try_into().unwrap();
        let linear: usize = self.origin + offset;
        linear % self.err.len()
    }
    fn next(&mut self) {
        self.err[self.origin] = 0;
        self.origin = self.index(1, 0);
    }
    fn get(&self) -> i16 {
        self.err[self.origin] / self.denominator
    }
    fn carry(&mut self, err: i16) {
        for (dx, dy, mul) in &self.diffusion {
            let i = self.index(*dx, *dy);
            self.err[i] += mul * err;
        }
    }
    fn pixel(&mut self, grey: u8) -> PixelColor {
        let grey: i16 = grey as i16 + self.get();
        if grey < Dither::THRESHOLD {
            self.carry(grey);
            PixelColor::Dark
        } else {
            self.carry(grey - u8::MAX as i16);
            PixelColor::Light
        }
    }
    pub fn dither(&mut self, image: &Img) -> Vec<Word> {
        let bits_per_word: u32 = BITS_PER_WORD.try_into().unwrap();
        let (width, height, _) = image.size();
        self.provision(width.try_into().unwrap());
        let mut words: Vec<Word> = Vec::with_capacity((1 + width / BITS_PER_WORD) * height);
        for y in 0..height {
            let (mut w, mut word): (Word, Word) = (0, 0);
            for x in 0..width {
                let color = match image.get(x, y) {
                    Some(grey) => self.pixel(*grey),
                    None => PixelColor::Dark,
                };
                word = word | ((color as u32) << w);
                w += 1;
                if w >= bits_per_word {
                    words.push(word);
                    (w, word) = (0, 0);
                }
                self.next();
            }
            words.push(word);
        }
        words
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
