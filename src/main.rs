use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use esp_idf_hal::{self as hal, prelude::*, gpio::*};
use esp32_nimble::*;
use anyhow::Result;

fn main() ->Result<()>{
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();


    let peripherals = Peripherals::take().unwrap();

    
    Ok(())
}
