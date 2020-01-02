

pub struct Image<T> {
    size: (u32, u32),
    pixels: Vec<T>
}

pub type L<T> = [T; 1];
pub type LA<T> = [T; 2];
pub type RGB<T> = [T; 3];
pub type RGBA<T> = [T; 4];

