use crate::*;
pub fn run(a: &mut App, n: usize, stream: bool, load_ms: f64) -> Result<(), String> {
    let (w, h) = a.size;
    let mut prev = None;
    let mut raster = std::time::Duration::ZERO;
    let mut encode = raster;
    let mut output = raster;
    let mut bytes = 0usize;
    let mut checksum = 0u64;
    let yaw = a.view.yaw;
    let mut stdout = io::stdout().lock();
    for i in 0..n {
        a.view.yaw = yaw + i as f64 * 0.5;
        let start = Instant::now();
        let f = render(&a.scene, &a.view, w, h, None, None);
        raster += start.elapsed();
        let start = Instant::now();
        let mut ansi = String::from("\x1b[?2026h");
        f.diff(prev.as_ref(), &mut ansi);
        ansi.push_str("\x1b[?2026l");
        encode += start.elapsed();
        bytes += ansi.len();
        if stream {
            let start = Instant::now();
            stdout
                .write_all(ansi.as_bytes())
                .and_then(|_| stdout.flush())
                .map_err(|e| e.to_string())?;
            output += start.elapsed()
        }
        checksum = checksum.wrapping_add(
            f.rgb
                .iter()
                .map(|c| c[0] as u64 + c[1] as u64 * 3 + c[2] as u64 * 7)
                .sum::<u64>(),
        );
        prev = Some(f)
    }
    let batch = Instant::now();
    a.tab = a.target_order();
    let order_us = batch.elapsed().as_secs_f64() * 1e6;
    let start = Instant::now();
    for _ in 0..1000 {
        a.cycle(false);
        std::hint::black_box(&a.view);
    }
    let switch_us = start.elapsed().as_secs_f64() * 1000.;
    let result=format!("frames={n} load_ms={load_ms:.3} raster_ms={:.3} fps={:.2} ansi_ms={:.3} write_ms={:.3} ansi_bytes={bytes} order_us={order_us:.3} switch_us={switch_us:.3} checksum={checksum}",raster.as_secs_f64()*1000.,n as f64/raster.as_secs_f64(),encode.as_secs_f64()*1000.,output.as_secs_f64()*1000.);
    if stream {
        eprintln!("{result}")
    } else {
        writeln!(stdout, "{result}").map_err(|e| e.to_string())?
    }
    Ok(())
}
