
use std::sync::Arc;

use esp_idf_sys as _; // If using the `binstart` feature of `esp-idf-sys`, always keep this module imported
use esp_idf_hal::{self as hal, prelude::*, gpio::*};
use esp32_nimble::{*, hid::*, utilities::mutex::*};
use anyhow::Result;
use hal::adc::*;
use tm1637::*;

const GAMEPAD_ID:u8 = 0x01;

const GAMEPAD_REPORT_DESCRIPTOR:&[u8] = hid!(
    (USAGE_PAGE, 0x01),                 // Generic Desktop
    (USAGE, 0x05),                      // Gamepad
    (COLLECTION, 0x01),                 // Application
        (REPORT_ID, GAMEPAD_ID), 
        (USAGE, 0x97),                  // Thumbstick
        (COLLECTION, 0x00),             // Physical
            (USAGE_PAGE, 0x01),         // Generic Desktop
            (USAGE, 0x30),              // X
            (USAGE, 0x31),              // Y
            (USAGE, 0x33),              // Rx
            (USAGE, 0x34),              // Ry
            (LOGICAL_MINIMUM, 0x00, 0x00),    // 0
            (LOGICAL_MAXIMUM, 0x92, 0x09),    // 2450
            (REPORT_SIZE, 0x10),        // 16 bits per axes
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

            (USAGE_PAGE,0x01),          // Generic Desktop
            (USAGE, 0x39),              // Hat Switch
            (LOGICAL_MINIMUM, 0x01),
            (LOGICAL_MAXIMUM, 0x08),
            (REPORT_SIZE, 0x04),        // 4 bit
            (REPORT_COUNT, 0x42),
            (HIDINPUT, 0x02),           
        (END_COLLECTION),
    (END_COLLECTION),
);

#[repr(packed)]
struct GamepadReport{
    x:u16,
    y:u16,
    rx:u16,
    ry:u16,
    buttons:u16,
}

/// is this a good way of doing it? idk
struct GamepadAxis<'a>{
    x: AdcChannelDriver<'a,Gpio39,Atten11dB<ADC1>>,
    y: AdcChannelDriver<'a,Gpio36,Atten11dB<ADC1>>,
    rx: AdcChannelDriver<'a,Gpio35,Atten11dB<ADC1>>,
    ry: AdcChannelDriver<'a,Gpio34,Atten11dB<ADC1>>,
}

struct GamepadButtons<'a>{
    // output pin groups
    select_0: PinDriver<'a, Gpio15, Output>,
    select_1: PinDriver<'a, Gpio2, Output>,

    // input pin
    group_0: PinDriver<'a, Gpio17, Input>,
    group_1: PinDriver<'a, Gpio16, Input>,
    group_2: PinDriver<'a, Gpio4, Input>,
    group_3: PinDriver<'a, Gpio12, Input>,
}

impl <'a> GamepadButtons <'a>{
    fn read_value(&mut self, group:u16, button:u16)->Result<bool>{
        match button{
            0=>{self.select_1.set_low()?; self.select_0.set_low()?; },
            1=>{self.select_1.set_low()?; self.select_0.set_high()?; },
            2=>{self.select_1.set_high()?; self.select_0.set_low()?; },
            3=>{self.select_1.set_high()?; self.select_0.set_high()?; },
            _=>unreachable!()
        }
        // need a delay here for the demultiplexer ~200 nanoseconds
        hal::delay::Ets::delay_us(2);
        match group{
            0=>Ok(self.group_0.is_low()),
            1=>Ok(self.group_1.is_low()),
            2=>Ok(self.group_2.is_low()),
            3=>Ok(self.group_3.is_low()),
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
        select_pins: (Gpio15, Gpio2), 
        input_groups: (Gpio17, Gpio16, Gpio4, Gpio12),
        adc_pins: (Gpio39, Gpio36, Gpio35, Gpio34)
    )->Result<Self>
    {
        let mut buttons = GamepadButtons { 
            select_0: PinDriver::output(select_pins.0)?, 
            select_1: PinDriver::output(select_pins.1)?, 

            group_0: PinDriver::input(input_groups.0)?, 
            group_1: PinDriver::input(input_groups.1)?, 
            group_2: PinDriver::input(input_groups.2)?, 
            group_3: PinDriver::input(input_groups.3)? 
        };
        buttons.group_0.set_pull(Pull::Up)?;
        buttons.group_1.set_pull(Pull::Up)?;
        buttons.group_2.set_pull(Pull::Up)?;
        buttons.group_3.set_pull(Pull::Up)?;
        Ok(Self {
            gamepad,
            buttons,
            adc:AdcDriver::new(adc, &AdcConfig::default().calibration(true))?, 
            axis: GamepadAxis {
                x: AdcChannelDriver::new(adc_pins.0)?, 
                y:AdcChannelDriver::new(adc_pins.1)?, 
                rx: AdcChannelDriver::new(adc_pins.2)?, 
                ry: AdcChannelDriver::new(adc_pins.3)? 
            }, 
            report: GamepadReport { 
                x: 1680, 
                y: 1680, 
                rx: 1680, 
                ry: 1680, 
                buttons: 0 
            }
        })
    }
    pub fn read(&mut self)->Result<()>{
        self.report.x = self.adc.read(&mut self.axis.x)?;
        self.report.y = self.adc.read(&mut self.axis.y)?;
        self.report.rx = self.adc.read(&mut self.axis.rx)?;
        self.report.ry = self.adc.read(&mut self.axis.ry)?;
        // iterate through each button and set the correct bit in self.report.buttons for it
        self.report.buttons = 0;
        for group in 0..=2{
            for button in 0..=3{
                self.report.buttons |= (self.buttons.read_value(group, button)? as u16)<<(group*4 + button);
            }
        }
        let up = self.buttons.read_value(3, 0)?;
        let right = self.buttons.read_value(3, 1)?;
        let down = self.buttons.read_value(3, 2)?;
        let left = self.buttons.read_value(3, 3)?;
        let mut val = 0u16;
        if up{
            if left{
                val = 8;
            }
            else if right{
                val = 2;
            }
            else{
                val = 1;
            }
        }
        else if down{
            if left{
                val = 6;
            }
            else if right{
                val = 4;
            }
            else{
                val = 5;
            }
        }
        else if left{
            val = 7;
        }
        else if right{
            val = 3;
        }

        self.report.buttons |= val<<12;
        self.gamepad.lock().set_from(&self.report).notify();
        Ok(())
    }
}


fn main() ->Result<()>{
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_sys::link_patches();

    let peripherals = Peripherals::take().unwrap();

    let dev = BLEDevice::take();
    dev.set_power(enums::PowerType::Default, enums::PowerLevel::P9).expect("Power not set");
    dev.security().set_io_cap(enums::SecurityIOCap::NoInputNoOutput).set_auth(false, false, true);

    let server = dev.get_server();
    let mut hid_device = BLEHIDDevice::new(server);
    let input = hid_device.input_report(GAMEPAD_ID);

    hid_device.manufacturer("Clueninja");
    hid_device.pnp(0x02, 0x05ac, 0x820a, 0x0210);
    hid_device.report_map(GAMEPAD_REPORT_DESCRIPTOR);
    hid_device.hid_info(0x00, 0x01);
    hid_device.set_battery_level(100);


    let adv = dev.get_advertising();
    adv.name("Esp Gamepad")
        .appearance(0x03C4)
        .add_service_uuid(hid_device.hid_service().lock().uuid())
        .scan_response(true);
    adv.start().unwrap();
    
    
    let mut gamepad = Gamepad::new(
        input,
        peripherals.adc1,
        (
            peripherals.pins.gpio15,
            peripherals.pins.gpio2,
        ),
        (
            peripherals.pins.gpio17,
            peripherals.pins.gpio16,
            peripherals.pins.gpio4,
            peripherals.pins.gpio12
        ),
        (
            peripherals.pins.gpio39,
            peripherals.pins.gpio36,
            peripherals.pins.gpio35,
            peripherals.pins.gpio34
        )
    )?; 
/* 
    // Custom driver seems to work fine
    let mut clk = PinDriver::output(peripherals.pins.gpio14)?;
    let mut dio = PinDriver::input_output(peripherals.pins.gpio13)?;
    let mut binding = hal::delay::Ets;
    let mut tm = TM1637::new(
        &mut clk, 
        &mut dio,
        &mut binding);
    tm.init().expect("TM1637 IC failed to initialise");
    tm.set_brightness(7).expect("Setting Brightness failed");
 
    let mut counter:u16 = 0;
    */
    loop{
        if server.connected_count()>0{
            gamepad.read()?;
            //tm.clear().unwrap();
            //hal::delay::FreeRtos::delay_ms(10);
        }
        else{
            hal::delay::FreeRtos::delay_ms(200);
        
            //tm.print_hex(0, &[(((counter/100)/10) as u8 & 0x0f), (((counter/100)%10) as u8 & 0x0f), ((counter/10)as u8) & 0x0f, ((counter%10)as u8)&0x0f ]).expect("printing wait time failed");
            //counter +=1;

        }
    }
}
