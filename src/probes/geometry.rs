use crate::parser::Event;
use crate::probe::{Category, Probe, ProbeStatus};

pub fn probes() -> Vec<Probe> {
    vec![
        // Response: CSI 6 ; height ; width t
        Probe::new(
            "cell-size-pixels",
            Category::Geometry,
            b"\x1b[16t".to_vec(),
            Box::new(|events| {
                for event in events {
                    if let Event::WindowOp { op, params } = event
                        && *op == 6
                        && params.len() >= 2
                    {
                        return (
                            ProbeStatus::Supported,
                            Some(format!("{}x{} px", params[1], params[0])),
                        );
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        ),
        // Response: CSI 8 ; rows ; cols t
        Probe::new(
            "text-area-cells",
            Category::Geometry,
            b"\x1b[18t".to_vec(),
            Box::new(|events| {
                for event in events {
                    if let Event::WindowOp { op, params } = event
                        && *op == 8
                        && params.len() >= 2
                    {
                        return (
                            ProbeStatus::Supported,
                            Some(format!("{} cols x {} rows", params[1], params[0])),
                        );
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        ),
        // Response: CSI 4 ; height ; width t
        Probe::new(
            "text-area-pixels",
            Category::Geometry,
            b"\x1b[14t".to_vec(),
            Box::new(|events| {
                for event in events {
                    if let Event::WindowOp { op, params } = event
                        && *op == 4
                        && params.len() >= 2
                    {
                        return (
                            ProbeStatus::Supported,
                            Some(format!("{}x{} px", params[1], params[0])),
                        );
                    }
                }
                (ProbeStatus::Unknown, None)
            }),
        ),
    ]
}
