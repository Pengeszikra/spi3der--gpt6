use crate::scene::*;
#[derive(Clone, Debug)]
pub struct View {
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    pub zoom: f64,
    pub center: V,
    pub dist: f64,
    pub radius: f64,
    pub ortho: bool,
    pub sky: bool,
    pub aspect: f64,
    pub edges: bool,
}
impl Default for View {
    fn default() -> Self {
        Self {
            yaw: -34.,
            pitch: 26.,
            roll: 0.,
            zoom: 1.,
            center: [0.; 3],
            dist: 4.15,
            radius: 1.,
            ortho: false,
            sky: false,
            aspect: 1.,
            edges: true,
        }
    }
}
impl View {
    pub fn home(scene: &Scene, w: usize, h: usize, aspect: f64) -> Self {
        let mut v = Self {
            sky: scene.sky,
            aspect,
            ..Self::default()
        };
        if scene.sky {
            let start = scene.root.get("start");
            v.center = vec3(start.get("eye")).unwrap_or_default();
            v.yaw = start.get("yaw").num();
            v.pitch = start.get("pitch").num();
            v.dist = 0.;
            return v;
        }
        let pts = scene.objects.iter().flat_map(|o| o.p.iter());
        let (center, r) = bounds(pts.clone());
        v.center = center;
        v.radius = r.max(1e-10);
        v.dist = v.radius * 1.25 / 17.5f64.to_radians().sin();
        let c = v.camera(w, h);
        let mut ext: [f64; 2] = [0.; 2];
        for p in pts {
            let q = c.project(*p);
            ext[0] = ext[0].max((q[0] - w as f64 / 2.).abs());
            ext[1] = ext[1].max((q[1] - h as f64 / 2.).abs());
        }
        if ext.iter().all(|x| *x > 0.) {
            let z = (0.45 * w as f64 / ext[0])
                .min(0.45 * h as f64 / ext[1])
                .clamp(0.05, 1e4);
            v.dist /= z;
            v.radius /= z
        }
        v
    }
    pub fn basis(&self) -> [V; 3] {
        let (s, c) = self.yaw.to_radians().sin_cos();
        let (p, q) = self.pitch.to_radians().sin_cos();
        let r = [c, 0., s];
        let u = [s * p, q, -c * p];
        let (sr, cr) = self.roll.to_radians().sin_cos();
        [
            [s * q, -p, -c * q],
            add(mul(r, cr), mul(u, -sr)),
            add(mul(u, cr), mul(r, sr)),
        ]
    }
    pub fn eye(&self) -> V {
        add(self.center, mul(self.basis()[0], -self.dist / self.zoom))
    }
    pub fn camera(&self, w: usize, h: usize) -> Camera {
        let [f, r, u] = self.basis();
        let size = if self.sky {
            (w as f64).max(h as f64 * self.aspect)
        } else {
            (w as f64).min(h as f64 * self.aspect)
        };
        let k = if self.ortho {
            size * 0.4 * self.zoom / self.radius
        } else {
            let fov = if self.sky {
                (60. / self.zoom).clamp(0.5, 150.)
            } else {
                35.
            };
            size * 0.5 / (fov * 0.5).to_radians().tan()
        };
        Camera {
            f,
            r,
            u,
            eye: self.eye(),
            k,
            ortho: self.ortho,
            w: w as f64,
            h: h as f64,
            aspect: self.aspect,
        }
    }
    pub fn aim(&mut self, at: V) {
        let eye = self.eye();
        let d = sub(at, eye);
        let l = len(d);
        if l < 1e-25 {
            return;
        }
        self.yaw = d[0].atan2(-d[2]).to_degrees();
        self.pitch = -(d[1] / l).clamp(-1., 1.).asin().to_degrees();
        self.center = at;
        self.dist = l * self.zoom
    }
    pub fn free(&mut self) {
        self.center = self.eye();
        self.dist = 0.;
    }
}
pub struct Camera {
    pub f: V,
    pub r: V,
    pub u: V,
    pub eye: V,
    pub k: f64,
    pub ortho: bool,
    pub w: f64,
    pub h: f64,
    pub aspect: f64,
}
impl Camera {
    pub fn project(&self, p: V) -> V {
        let d = sub(p, self.eye);
        let z = dot(d, self.f);
        let k = if self.ortho { self.k } else { self.k / z };
        [
            self.w / 2. + dot(d, self.r) * k,
            self.h / 2. - dot(d, self.u) * k / self.aspect,
            z,
        ]
    }
    pub fn point(&self, x: f64, y: f64, z: f64) -> V {
        let k = if self.ortho { self.k } else { self.k / z };
        add(
            self.eye,
            add(
                mul(self.f, z),
                add(
                    mul(self.r, (x - self.w / 2.) / k),
                    mul(self.u, -(y - self.h / 2.) * self.aspect / k),
                ),
            ),
        )
    }
}
#[derive(Default)]
pub struct Flight {
    pub roll_keys: [bool; 2],
    pub steer: bool,
    pub arming: Option<f64>,
    pub galaxy_brake: bool,
    pub unit: f64,
    pub levels: [i32; 3],
    pub frozen: bool,
    pub orbit: bool,
    pub throttle: f64,
    pub spin: bool,
    pub cruise: V,
    pub gravity: Option<(V, V)>,
}
impl Flight {
    pub fn active(&self) -> bool {
        !self.frozen
            && (self.spin
                || self.roll_keys[0] != self.roll_keys[1]
                || self.steer
                || self.arming.is_some()
                || self.levels != [0; 3]
                || self.cruise != [0.; 3])
    }
    pub fn pace(n: i32) -> f64 {
        (n.signum() as f64) * (1.7f64.powi(n.abs()) - 1.) / 0.7
    }
    fn velocity(&self, v: &View) -> V {
        let b = v.basis();
        let [f, r, u] = self.levels.map(Self::pace);
        add(
            self.cruise,
            mul(
                add(mul(b[0], f), add(mul(b[1], r), mul(b[2], u))),
                self.unit * self.throttle,
            ),
        )
    }
    pub fn gravity_at(&mut self, v: &View, at: V) {
        let radial = sub(v.eye(), at);
        let speed = self.velocity(v);
        let n = cross(radial, speed);
        if len(n) > 1e-9 * len(radial) * len(speed) {
            self.gravity = Some((at, mul(n, 1. / len(n))))
        }
    }
    pub fn speed(&mut self, i: usize, step: i32) {
        self.levels[i] = (self.levels[i] + step).clamp(-60, 60);
        self.throttle = 1.
    }
    pub fn stop(&mut self) {
        self.arming = None;
        self.roll_keys = [false; 2];
        self.steer = false;
        self.galaxy_brake = false;
        self.throttle = 1.;
        self.levels = [0; 3];
        self.spin = false;
        self.orbit = false;
        self.cruise = [0.; 3];
        self.gravity = None;
    }
    pub fn tick(&mut self, v: &mut View, s: &Scene, target: Option<usize>, dt: f64) -> bool {
        if !self.active() || dt <= 0. {
            return false;
        }
        if s.sky {
            let direction = i32::from(self.roll_keys[1]) - i32::from(self.roll_keys[0]);
            v.roll = (v.roll + f64::from(direction) * 60. * dt).rem_euclid(360.);
        }
        if self.spin {
            v.yaw += 24. * dt
        }
        let obj = target.and_then(|i| s.objects.get(i));
        if obj.is_none() {
            self.steer = false;
            self.arming = None;
        }
        if let Some(t) = self.arming {
            if let Some(o) = obj {
                if t < 0. {
                    let decay = (-6. * dt).exp();
                    self.throttle *= decay;
                    self.cruise = mul(self.cruise, decay);
                    if len(self.velocity(v)) < 0.005 {
                        self.levels = [0; 3];
                        self.cruise = [0.; 3];
                        self.throttle = 1.;
                        self.arming = Some(0.)
                    }
                } else if t + dt >= 1. {
                    v.aim(o.center);
                    self.orbit = true;
                    self.steer = false;
                    self.arming = None
                } else {
                    self.arming = Some(t + dt)
                }
            }
        }
        if let Some(o) =
            obj.filter(|_| s.sky && !self.orbit && (self.steer || self.gravity.is_none()))
        {
            v.free();
            let d = sub(o.center, v.center);
            let l = len(d);
            if l > 1e-25 {
                let yaw = d[0].atan2(-d[2]).to_degrees();
                let pitch = -(d[1] / l).clamp(-1., 1.).asin().to_degrees();
                let dy = (yaw - v.yaw + 180.).rem_euclid(360.) - 180.;
                let dp = pitch - v.pitch;
                let k = 1. - (-3. * dt).exp();
                if dp.abs() + dy.abs() < 0.005 {
                    v.yaw = yaw;
                    v.pitch = pitch;
                    self.steer = false
                } else {
                    v.yaw += dy * k;
                    v.pitch += dp * k;
                    self.steer = true
                }
            }
        }
        if self.levels == [0; 3] && self.cruise == [0.; 3] {
            return true;
        }
        let [f, r, u] = self.levels.map(Self::pace);
        if let Some(o) = obj.filter(|_| !s.sky || self.orbit) {
            v.center = o.center;
            v.dist = (v.dist * (-f * 0.09 * dt).exp()).max(o.radius * 1.05 * v.zoom);
            if f > 0. && v.dist <= o.radius * 1.05 * v.zoom {
                self.levels[0] = 0
            }
            v.yaw -= (r * 0.175 * dt).to_degrees();
            v.pitch = (v.pitch + (u * 0.175 * dt).to_degrees()).rem_euclid(360.);
        } else {
            v.free();
            let basis = v.basis();
            let mut unit = 0.25;
            if s.sky {
                let mut near = s
                    .objects
                    .iter()
                    .filter(|o| o.star.as_ref().is_some_and(|star| !star.far))
                    .map(|o| (len(sub(o.center, v.center)) - o.radius).max(0.))
                    .fold(f64::INFINITY, f64::min);
                if let Some(o) = obj.filter(|o| o.star.as_ref().is_some_and(|star| !star.far)) {
                    near = near.max(len(sub(o.center, v.center)))
                }
                unit *= (near * 0.2).clamp(1e-14, 1.)
            }
            self.unit = unit;
            let mut speed = mul(
                add(mul(basis[0], f), add(mul(basis[1], r), mul(basis[2], u))),
                unit,
            );
            if let Some(o) = obj.filter(|o| {
                s.sky && self.arming.is_none() && dot(sub(o.center, v.center), speed) > 0.
            }) {
                let rest = (len(sub(o.center, v.center)) - 1.05 * o.radius).max(0.);
                let full = len(speed);
                let galaxy = o.star.as_ref().is_some_and(|star| star.galaxy);
                if galaxy && full * self.throttle >= 1e5 {
                    self.galaxy_brake = true
                }
                if full > 0. && (!galaxy || self.galaxy_brake && rest > 0.) {
                    let want = (rest / 2.).max(0.5_f64.min(rest * 4.)) / full;
                    self.throttle +=
                        (want.min(self.throttle) - self.throttle) * (1. - (-4. * dt).exp());
                    self.throttle = self.throttle.min(rest / (0.6 * full));
                    if rest <= (o.radius * 0.02).max(1e-14) {
                        self.stop();
                        return true;
                    }
                }
            }
            speed = add(mul(speed, self.throttle), self.cruise);
            if let Some((center, axis)) = self.gravity {
                let radial = sub(v.center, center);
                let tangent = norm(cross(axis, radial));
                let angle = dot(speed, tangent) * dt / len(radial).max(1e-20);
                if dot(speed, tangent) > 1e-4 * unit && len(radial) > 1e-18 {
                    let rotate = |p: V| {
                        add(
                            add(mul(p, angle.cos()), mul(cross(axis, p), angle.sin())),
                            mul(axis, dot(axis, p) * (1. - angle.cos())),
                        )
                    };
                    v.center = add(
                        center,
                        add(
                            rotate(radial),
                            mul(sub(speed, mul(tangent, dot(speed, tangent))), dt),
                        ),
                    );
                    self.cruise = rotate(self.cruise);
                    let dir = rotate(basis[0]);
                    v.yaw = dir[0].atan2(-dir[2]).to_degrees();
                    v.pitch = -dir[1].clamp(-1., 1.).asin().to_degrees();
                    return true;
                } else {
                    self.gravity = None
                }
            }
            v.center = add(v.center, mul(speed, dt));
            if obj.is_some() {
                self.steer = true
            }
        }
        true
    }
}
