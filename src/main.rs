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
struct GamepadAxis<'a>{
    X: AdcChannelDriver<'a,Gpio32,Atten11dB<ADC1>>,
    Y: AdcChannelDriver<'a,Gpio33,Atten11dB<ADC1>>,
    Rx: AdcChannelDriver<'a,Gpio34,Atten11dB<ADC1>>,
    Ry: AdcChannelDriver<'a,Gpio35,Atten11dB<ADC1>>,
}
enum ButtonGroup{
    RightThumb,
    LeftThumb,
    Trigger,
    Home
}
struct GamepadButtons<'a>{
    // output pin groups
    right_thumb: PinDriver<'a, Gpio10, Output>,
    left_thumb: PinDriver<'a, Gpio11, Output>,
    trigger: PinDriver<'a, Gpio12, Output>,
    home: PinDriver<'a, Gpio13, Output>,

    // input pin
    button_1: PinDriver<'a, Gpio14, Input>,
    button_2: PinDriver<'a, Gpio15, Input>,
    button_3: PinDriver<'a, Gpio16, Input>,
    button_4: PinDriver<'a, Gpio17, Input>,
}

impl <'a> GamepadButtons <'a>{
    fn read_value(&mut self, group:u16, button:u16)->bool{
        self.right_thumb.set_low().unwrap();
        self.left_thumb.set_low().unwrap();
        self.trigger.set_low().unwrap();
        self.home.set_low().unwrap();
        match group{
            1=>{
                self.right_thumb.set_high().unwrap();
            },
            2=>{
                self.left_thumb.set_high().unwrap();
            },
            3=>{
                self.trigger.set_high().unwrap();
            },
            4=>{
                self.home.set_high().unwrap();
            },
            _=>unreachable!()
        }
        match button {
            1=> self.button_1.is_high(),
            2=> self.button_2.is_high(),
            3=> self.button_3.is_high(),
            4=> self.button_4.is_high(),
            _=>unreachable!()
        }
    }
}

struct Gamepad<'a>{
    gamepad : Arc<Mutex<BLECharacteristic>>,
    pub buttons: GamepadButtons<'a>,
    adc: AdcDriver<'a, ADC1>,
    pub axis: GamepadAxis<'a>,
    report:GamepadReport,
}

impl <'a> Gamepad<'a> 
    {
    pub fn new(
        gamepad:Arc<Mutex<BLECharacteristic>>, 
        adc: ADC1, 
        output_groups: (Gpio10, Gpio11, Gpio12, Gpio13), 
        input_groups: (Gpio14, Gpio15, Gpio16, Gpio17),
        adc_pins: (Gpio32, Gpio33, Gpio34, Gpio35)
    )->Self{
        Self {
            gamepad,
            buttons: GamepadButtons { 
                right_thumb: PinDriver::output(output_groups.0).unwrap(), 
                left_thumb: PinDriver::output(output_groups.1).unwrap(), 
                trigger: PinDriver::output(output_groups.2).unwrap(), 
                home: PinDriver::output(output_groups.3).unwrap(), 
                button_1: PinDriver::input(input_groups.0).unwrap(), 
                button_2: PinDriver::input(input_groups.1).unwrap(), 
                button_3: PinDriver::input(input_groups.2).unwrap(), 
                button_4: PinDriver::input(input_groups.3).unwrap() 
            },
            adc:AdcDriver::new(adc, &AdcConfig::default().calibration(true)).unwrap(), 
            axis: GamepadAxis {
                X: AdcChannelDriver::new(adc_pins.0).unwrap(), 
                Y:AdcChannelDriver::new(adc_pins.1).unwrap(), 
                Rx: AdcChannelDriver::new(adc_pins.2).unwrap(), 
                Ry: AdcChannelDriver::new(adc_pins.3).unwrap() 
            }, 
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
        self.report.X = self.adc.read(&mut self.axis.X).unwrap() as u8;
        self.report.Y = self.adc.read(&mut self.axis.Y).unwrap() as u8;
        self.report.Rx = self.adc.read(&mut self.axis.Rx).unwrap() as u8;
        self.report.Ry = self.adc.read(&mut self.axis.Ry).unwrap() as u8;

        // iterate through each button and set the correct bit in self.report.buttons for it
        self.report.buttons = 0;
        for group in 1..=4{
            for button in 1..=4{
                self.report.buttons |= self.buttons.read_value(group, button) as u16;
            }
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
    let mut gamepad = Gamepad::new(
        hid_device.input_report(GAMEPAD_ID), 
        peripherals.adc1,
        (
            peripherals.pins.gpio10,
            peripherals.pins.gpio11,
            peripherals.pins.gpio12,
            peripherals.pins.gpio13
        ),
        (
            peripherals.pins.gpio14,
            peripherals.pins.gpio15,
            peripherals.pins.gpio16,
            peripherals.pins.gpio17
        ),
        (
            peripherals.pins.gpio32,
            peripherals.pins.gpio33,
            peripherals.pins.gpio34,
            peripherals.pins.gpio35
        )
    );
    // hard code gpio pins to use, also pin matrix won't work in this configuration
    
    loop{
        if server.connected_count()>0{
            gamepad.read();
        }
    }

    Ok(())
}
