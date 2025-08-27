use egui::{ImageSource, Vec2, include_image};
pub struct InstanceGraphics {
    // TODO: Figure out what is the correct way to deal with images
    pub svg: ImageSource<'static>,
    pub pins: &'static [PinInfo],
}

#[derive(Debug, Clone, Copy)]
pub enum PinKind {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy)]
pub struct PinInfo {
    pub kind: PinKind,
    pub offset: Vec2,
}

pub static NAND_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/nand.svg"),
    // TODO: offset must be made from the base_gate_size otherwise it will be unaligned when gates resize
    pins: &[
        PinInfo {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinInfo {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
        PinInfo {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
    ],
};

pub static AND_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/and.svg"),
    pins: &[
        PinInfo {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinInfo {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
        PinInfo {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
    ],
};

pub static POWER_ON_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/switch-on.svg"),
    pins: &[PinInfo {
        kind: PinKind::Output,
        offset: Vec2::new(40.0, 0.0),
    }],
};

pub static POWER_OFF_GRAPHICS: InstanceGraphics = InstanceGraphics {
    svg: include_image!("../assets/switch-off.svg"),
    pins: &[PinInfo {
        kind: PinKind::Output,
        offset: Vec2::new(40.0, 0.0),
    }],
};
