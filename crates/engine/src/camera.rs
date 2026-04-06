use glam::{Mat4, Vec2};

/// 2D ortografická kamera s podporou posouvání a zoomování.
///
/// Souřadnicový systém: (0,0) vlevo nahoře, x doprava, y dolů (stejný jako obrazovka).
/// Kamera definuje, jaká část herního světa je viditelná.
pub struct Camera {
    /// Pozice středu pohledu ve světových souřadnicích.
    pub position: Vec2,
    /// Úroveň přiblížení (1.0 = 1:1, 2.0 = dvojnásobné přiblížení).
    pub zoom: f32,
    /// Rozlišení viewportu v pixelech.
    viewport: Vec2,
}

impl Camera {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Self {
            position: Vec2::ZERO,
            zoom: 1.0,
            viewport: Vec2::new(viewport_width, viewport_height),
        }
    }

    /// Aktualizuje rozlišení viewportu (volat při resize okna).
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = Vec2::new(width, height);
    }

    /// Vrátí view-projection matici pro GPU.
    ///
    /// Používá ortografickou projekci:
    ///   levý kraj = position.x - (viewport.x / 2) / zoom
    ///   pravý kraj = position.x + (viewport.x / 2) / zoom
    ///   atd.
    pub fn view_projection(&self) -> Mat4 {
        let half_w = self.viewport.x * 0.5 / self.zoom;
        let half_h = self.viewport.y * 0.5 / self.zoom;

        let left   = self.position.x - half_w;
        let right  = self.position.x + half_w;
        let top    = self.position.y - half_h;
        let bottom = self.position.y + half_h;

        // ortho_rh_zo: right-handed, zero-to-one depth (wgpu NDC)
        // top/bottom obráceno aby y=0 bylo nahoře (y dolů v herním světě)
        Mat4::orthographic_rh(left, right, bottom, top, -1.0, 1.0)
    }

    /// Posune kameru o `delta` herních pixelů.
    pub fn pan(&mut self, delta: Vec2) {
        self.position += delta;
    }

    /// Přiblíží/oddálí kameru kolem bodu `around` (v obrazovkových souřadnicích).
    pub fn zoom_around(&mut self, factor: f32, around: Vec2) {
        let world_before = self.screen_to_world(around);
        self.zoom = (self.zoom * factor).clamp(0.25, 8.0);
        let world_after = self.screen_to_world(around);
        self.position += world_before - world_after;
    }

    /// Převede obrazovkové souřadnice (pixely) na světové souřadnice.
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        let half = self.viewport * 0.5;
        (screen - half) / self.zoom + self.position
    }

    /// Převede světové souřadnice na obrazovkové pixely.
    pub fn world_to_screen(&self, world: Vec2) -> Vec2 {
        let half = self.viewport * 0.5;
        (world - self.position) * self.zoom + half
    }

    pub fn viewport(&self) -> Vec2 {
        self.viewport
    }
}

/// Uniform buffer pro GPU – musí odpovídat `CameraUniform` v shaderu.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [[f32; 4]; 4],
}

impl CameraUniform {
    pub fn from_camera(camera: &Camera) -> Self {
        Self {
            view_proj: camera.view_projection().to_cols_array_2d(),
        }
    }

    /// Ortografická projekce přímo na pixely obrazovky (pro UI).
    /// (0,0) vlevo nahoře, (width, height) vpravo dole.
    pub fn screen_space(width: f32, height: f32) -> Self {
        let mat = Mat4::orthographic_rh(0.0, width, height, 0.0, -1.0, 1.0);
        Self { view_proj: mat.to_cols_array_2d() }
    }
}
