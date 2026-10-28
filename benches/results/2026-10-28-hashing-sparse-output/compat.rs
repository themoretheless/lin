use std::{mem::MaybeUninit,sync::Arc};
fn main(){ let mut x=Arc::<[f32]>::new_uninit_slice(768);let slots=Arc::get_mut(&mut x).unwrap();slots.fill(MaybeUninit::new(0.0));slots[42].write(0.25);let x=unsafe{x.assume_init()};assert_eq!(x[42].to_bits(),0.25f32.to_bits());assert_eq!(x[0].to_bits(),0);assert_eq!(x.len(),768);}
