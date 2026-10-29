use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

#[allow(unreachable_pub)]
mod sha512_constants {
    include!("../core/src/sha512_constants.rs");
}

const TEMPLATE: &str = "../core/src/shaders/sha512_web.wgsl";
const ROUND_MARKER: &str = "    // @SHA512_UNROLLED_ROUNDS@";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for path in [TEMPLATE, "../core/src/sha512_constants.rs"] {
        println!("cargo:rerun-if-changed={path}");
    }
    println!("cargo:rerun-if-changed=build.rs");

    let output_dir =
        PathBuf::from(std::env::var_os("OUT_DIR").ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "OUT_DIR is not set")
        })?);
    let mut output = String::new();
    let mut state = ["a", "b", "c", "d", "e", "f", "g", "h"];

    for (round, constant) in sha512_constants::ROUND_CONSTANTS.iter().enumerate() {
        let slot = round & 15;
        if round >= 16 {
            writeln!(
                output,
                "    w{slot} = add64(add64(small_sigma1(w{}), w{}), add64(small_sigma0(w{}), w{slot}));",
                (round + 14) & 15,
                (round + 9) & 15,
                (round + 1) & 15
            )?;
        }
        let [a, b, c, d, e, f, g, h] = state;
        let high = constant >> 32;
        let low = constant & 0xffff_ffff;
        writeln!(
            output,
            "    let temp1_{round} = add64(add64(add64({h}, big_sigma1({e})), choice({e}, {f}, {g})), add64(vec2<u32>(0x{high:08x}u, 0x{low:08x}u), w{slot}));"
        )?;
        writeln!(
            output,
            "    let temp2_{round} = add64(big_sigma0({a}), majority({a}, {b}, {c}));"
        )?;
        writeln!(output, "    {d} = add64({d}, temp1_{round});")?;
        writeln!(output, "    {h} = add64(temp1_{round}, temp2_{round});")?;
        state = [h, a, b, c, d, e, f, g];
    }

    let high = sha512_constants::IV[0] >> 32;
    let low = sha512_constants::IV[0] & 0xffff_ffff;
    writeln!(
        output,
        "    return add64({}, vec2<u32>(0x{high:08x}u, 0x{low:08x}u));",
        state[0]
    )?;

    let template = fs::read_to_string(TEMPLATE)?;
    if template.matches(ROUND_MARKER).count() != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("shader template {TEMPLATE} must contain one round marker"),
        )
        .into());
    }
    fs::write(
        output_dir.join("sha512_web.wgsl"),
        template.replace(ROUND_MARKER, &output),
    )?;

    Ok(())
}
