use crate::parser::Event;
use crate::probe::{Category, Probe, ProbeStatus};

pub fn probes() -> Vec<Probe> {
    vec![
        Probe {
            label: None,
            name: "cell-size-pixels",
            category: Category::Geometry,
            is_sentinel: false,
            query: b"\x1b[16t".to_vec(),
            interpret: Box::new(|events| {
                // Response: CSI 6 ; height ; width t
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
        },
        Probe {
            label: None,
            name: "text-area-cells",
            category: Category::Geometry,
            is_sentinel: false,
            query: b"\x1b[18t".to_vec(),
            interpret: Box::new(|events| {
                // Response: CSI 8 ; rows ; cols t
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
        },
        Probe {
            label: None,
            name: "text-area-pixels",
            category: Category::Geometry,
            is_sentinel: false,
            query: b"\x1b[14t".to_vec(),
            interpret: Box::new(|events| {
                // Response: CSI 4 ; height ; width t
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
        },
    ]
}
