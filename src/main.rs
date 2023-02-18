use std::sync::Arc;

use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use esp_idf_hal::{self as hal, prelude::*, gpio::*};
use esp32_nimble::{*, hid::*, utilities::mutex::*};
use anyhow::Result;
use hal::adc::*;

const GAMEPAD_ID:u8 = 0x01;

// I dont think this will work
/*
    GAMEPAD
        BUTTONS
            A,B,X,Y
            UP, DOWN, LEFT, RIGHT
            SELECT, HOME
            THUMB LEFT, THUMB RIGHT
        3D GAME CONTROLLER
            Turn Right/Left
            Pitch Forward/Backward
            // Roll Right/Left
            Move Right/Left
            Move Forward/Backward

*/
const GAMEPAD_REPORT_DESCRIPTOR:&[u8] = hid!(
    (USAGE_PAGE, 0x01),                 // Generic Desktop
    (USAGE, 0x05),                      // Gamepad
    (COLLECTION, 0x01),                 // Application
        (COLLECTION, 0x00),             // Physical
            (REPORT_ID, GAMEPAD_ID), 
            (USAGE_PAGE, 0x01),         // Generic Desktop
            (USAGE, 0x30),              // X
            (USAGE, 0x31),              // Y
            (USAGE, 0x33),              // Rx
            (USAGE, 0x34),              // Ry
            (LOGICAL_MINIMUM, 0x00),    // 0
            (LOGICAL_MAXIMUM, 0xFF),    // 255
            (REPORT_SIZE, 0x08),        // 8 bits per axes
            (REPORT_COUNT, 0x04),       // 4 Axes
            (HIDINPUT, 0x02),           // Data, Var, Abs

            (USAGE_PAGE, 0x09),         // Button Page
            (USAGE_MINIMUM, 0x01),
            (USAGE_MAXIMUM, 0x0C),      // 12 buttons?? might need more context
            (LOGICAL_MINIMUM, 0x00),
            (LOGICAL_MAXIMUM, 0x01),
            (REPORT_SIZE, 0x01),
            (REPORT_COUNT, 0x0C),
            (HIDINPUT, 0x02),

            (REPORT_SIZE, 0x04),        // 4 bits of padding
            (REPORT_COUNT, 0x01),
            (HIDINPUT, 0x01),
        (END_COLLECTION),
    (END_COLLECTION),
);

#[repr(packed)]
struct GamepadReport{
    X:u8,
    Y:u8,
    Rx:u8,
    Ry:u8,
    buttons:u16,
}

/// is this a good way of doing it? idk
struct GamepadAxis<'a, T:ADCPin, A:Adc>{
    X: AdcChannelDriver<'a,T,Atten11dB<A>>,
    Y: AdcChannelDriver<'a,T,Atten11dB<A>>,
    Rx: AdcChannelDriver<'a,T,Atten11dB<A>>,
    Ry: AdcChannelDriver<'a,T,Atten11dB<A>>,
}

struct Gamepad<'a, P: Pin, T:ADCPin>{
    gamepad : Arc<Mutex<BLECharacteristic>>,
    pub buttons: Vec<PinDriver<'a, P, Input>>,
    adc: AdcDriver<'a, ADC1>,
    //axis: GamepadAxis<'a, T, ADC1>,
    pub axis: Vec<AdcChannelDriver<'a, T, Atten11dB<ADC1>>>,
    report:GamepadReport,
}

impl <'a, P:InputPin, T:ADCPin> Gamepad<'a, P, T> 
    where Atten11dB<ADC1>:Attenuation<<T as ADCPin>::Adc>
    {
    pub fn new(gamepad:Arc<Mutex<BLECharacteristic>>, adc: ADC1)->Self{
        Self {
            gamepad,
            buttons: Vec::new(), 
            adc:AdcDriver::new(adc, &AdcConfig::default().calibration(true)).unwrap(), 
            axis: Vec::new(), 
            report: GamepadReport { 
                X: 0, 
                Y: 0, 
                Rx: 0, 
                Ry: 0, 
                buttons: 0 
            }
        }
    }
    pub fn read(&mut self){
        if let Some(a) = self.axis.get_mut(0){
            self.report.X = self.adc.read(a).unwrap() as u8;
        }
        if let Some(a) = self.axis.get_mut(1){
            self.report.Y = self.adc.read(a).unwrap() as u8;
        }
        if let Some(a) = self.axis.get_mut(2){
            self.report.Rx = self.adc.read(a).unwrap() as u8;
        }
        if let Some(a) = self.axis.get_mut(3){
            self.report.Ry = self.adc.read(a).unwrap() as u8;
        }
        self.report.buttons = 0;
        for (count, b) in self.buttons.iter_mut().enumerate(){
            self.report.buttons |= (b.is_high()as u16) << count;
        }
        self.gamepad.lock().set_from(&self.report).notify();
    }
}


fn main() ->Result<()>{
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();



    let dev = BLEDevice::take();
    dev.security().set_auth(false, true, true).set_io_cap(enums::SecurityIOCap::NoInputNoOutput);

    let mut server = dev.get_server();
    let mut hid_device = BLEHIDDevice::new(server);
    hid_device.report_map(GAMEPAD_REPORT_DESCRIPTOR);
    hid_device.manufacturer("Clueninja");
    hid_device.set_battery_level(100);

    dev.get_advertising()
        .name("Esp Gamepad")
        .appearance(0x03C4)
        .add_service_uuid(hid_device.hid_service().lock().uuid());
    

    let peripherals = Peripherals::take().unwrap();
    let mut gamepad = Gamepad::new(hid_device.input_report(GAMEPAD_ID), peripherals.adc1);
    // hard code gpio pins to use, also pin matrix won't work in this configuration
    gamepad.buttons.push(PinDriver::input(peripherals.pins.gpio0)?);
    gamepad.buttons.push(PinDriver::input(peripherals.pins.gpio1)?);

    loop{
        if server.connected_count()>0{
            
        }
    }

    Ok(())
}
