#![no_std]
#[cfg(not(test))]
use core::panic::PanicInfo;
mod data;

fn main() {}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
