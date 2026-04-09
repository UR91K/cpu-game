//! CPU-based NTSC presenter compatible with the existing ShaderRenderer API.
//! Reference shader logic: ../../full-shader.md

use num_traits::{AsPrimitive, Num};
use std::ops::{Add, Sub};

pub mod blit;
pub mod composite;
pub mod renderer;

pub use renderer::ShaderRenderer;

/// A size with a width and height.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Size<T> {
	pub width: T,
	pub height: T,
}

impl<T> Size<T> {
	/// Create a new `Size<T>` with the given width and height.
	pub fn new(width: T, height: T) -> Self {
		Size { width, height }
	}
}

impl<T: AsPrimitive<f32>> Size<T> {
	/// Get the aspect ratio of the size.
	#[inline(always)]
	pub fn aspect_ratio(&self) -> f32 {
		self.width.as_() / self.height.as_()
	}
}

impl<T: Sub<Output = T>> Sub for Size<T> {
	type Output = Size<T>;

	fn sub(self, rhs: Self) -> Self::Output {
		Self {
			width: self.width - rhs.width,
			height: self.height - rhs.height,
		}
	}
}

impl<T: Sub<T, Output = T> + Copy> Sub<T> for Size<T> {
	type Output = Size<T>;

	fn sub(self, rhs: T) -> Self::Output {
		Self {
			width: self.width - rhs,
			height: self.height - rhs,
		}
	}
}

impl<T: Add<Output = T>> Add for Size<T> {
	type Output = Size<T>;

	fn add(self, rhs: Self) -> Self::Output {
		Self {
			width: self.width + rhs.width,
			height: self.height + rhs.height,
		}
	}
}

impl<T: Add<T, Output = T> + Copy> Add<T> for Size<T> {
	type Output = Size<T>;

	fn add(self, rhs: T) -> Self::Output {
		Self {
			width: self.width + rhs,
			height: self.height + rhs,
		}
	}
}

impl<T> From<Size<T>> for [f32; 4]
where
	T: Copy + AsPrimitive<f32>,
{
	/// Convert a `Size<T>` to a `vec4` uniform.
	fn from(value: Size<T>) -> Self {
		[
			value.width.as_(),
			value.height.as_(),
			1.0 / value.width.as_(),
			1.0 / value.height.as_(),
		]
	}
}

/// Trait for surface or texture objects that can fetch size.
pub trait GetSize<C: Num> {
	type Error;
	/// Fetch the size of the object.
	fn size(&self) -> Result<Size<C>, Self::Error>;
}

impl<T: GetSize<u32>> GetSize<f32> for T {
	type Error = T::Error;

	fn size(&self) -> Result<Size<f32>, Self::Error> {
		let size = <T as GetSize<u32>>::size(self)?;
		Ok(Size {
			width: size.width as f32,
			height: size.height as f32,
		})
	}
}
