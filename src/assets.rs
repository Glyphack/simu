use std::fmt::Display;

use egui::{ImageSource, Vec2, include_image};
pub struct InstanceGraphics {
    // TODO: Figure out what is the correct way to deal with images
    pub svg: ImageSource<'static>,
    pub pins: &'static [PinGraphics],
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinKind {
    Input,
    Output,
}

impl Display for PinKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Input => f.write_str("Input"),
            Self::Output => f.write_str("Output"),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinGraphics {
    pub kind: PinKind,
    pub offset: Vec2,
}

pub static NAND_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/nand.svg"),
    // TODO: offset must be made from the base_gate_size otherwise it will be unaligned when gates resize
    pins: &[
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinGraphics {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
    ],
};

pub static AND_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/and.svg"),
    pins: &[
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinGraphics {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
    ],
};

pub static OR_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/or.svg"),
    pins: &[
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinGraphics {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
    ],
};

pub static NOR_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/nor.svg"),
    pins: &[
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinGraphics {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
    ],
};

pub static XOR_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/xor.svg"),
    pins: &[
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinGraphics {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
    ],
};

pub static XNOR_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/xnor.svg"),
    pins: &[
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinGraphics {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
        PinGraphics {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
    ],
};

pub static POWER_ON_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/switch-on.svg"),
    pins: &[PinGraphics {
        kind: PinKind::Output,
        offset: Vec2::new(40.0, 0.0),
    }],
};

pub static POWER_OFF_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/switch-off.svg"),
    pins: &[PinGraphics {
        kind: PinKind::Output,
        offset: Vec2::new(40.0, 0.0),
    }],
};

pub static LAMP_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/led-lamp.svg"),
    pins: &[PinGraphics {
        kind: PinKind::Input,
        offset: Vec2::new(-40.0, 0.0),
    }],
};

pub static CLOCK_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/wave.svg"),
    pins: &[PinGraphics {
        kind: PinKind::Output,
        offset: Vec2::new(40.0, 0.0),
    }],
};
