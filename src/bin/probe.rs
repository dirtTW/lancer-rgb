use lancer::{detect, profile_for, Rgb, Smbus};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let colour = if args.len() >= 2 {
        let h = u32::from_str_radix(args[1].trim_start_matches('#'), 16).expect("bad hex colour");
        Rgb::new((h >> 16) as u8, (h >> 8) as u8, h as u8)
    } else {
        Rgb::new(255, 0, 0)
    };

    let bus = match Smbus::open() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let dims = detect(&bus);
    println!("found {} ENE module(s)", dims.len());
    if dims.is_empty() {
        std::process::exit(2);
    }

    for d in &dims {
        let profile = profile_for(&d.name);
        println!(
            "  0x{:02X}  {}  {} LEDs  colour_reg={:?}",
            d.address,
            d.name,
            d.leds,
            profile.as_ref().map(|p| format!("{:#06X}", p.colour_reg))
        );
    }

    for d in &dims {
        match bus.set_solid(d, colour) {
            Ok(()) => println!("  0x{:02X} set #{:02X}{:02X}{:02X}", d.address, colour.r, colour.g, colour.b),
            Err(e) => println!("  0x{:02X} FAILED: {e}", d.address),
        }
        match bus.save(d) {
            Ok(()) => println!("  0x{:02X} saved to module", d.address),
            Err(e) => println!("  0x{:02X} save FAILED: {e}", d.address),
        }
        match bus.read_colours(d, d.leds * 3) {
            Ok(c) => println!("  0x{:02X} readback: {:02X?}", d.address, c),
            Err(e) => println!("  0x{:02X} readback FAILED: {e}", d.address),
        }
    }
}
