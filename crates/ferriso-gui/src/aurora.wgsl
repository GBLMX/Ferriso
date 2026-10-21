struct Uniforms {
    resolution: vec2<f32>,
    time: f32,
    intensity: f32,
    abyss: vec4<f32>,
    glow: vec4<f32>,
    accent: vec4<f32>,
    highlight: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );

    return vec4<f32>(positions[index], 0.0, 1.0);
}

fn hash21(p: vec2<f32>) -> f32 {
    var q = fract(vec3<f32>(p.x, p.y, p.x) * 0.1031);
    q += dot(q, vec3<f32>(q.y, q.z, q.x) + 33.33);
    return fract((q.x + q.y) * q.z);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);

    let a = hash21(i);
    let b = hash21(i + vec2<f32>(1.0, 0.0));
    let c = hash21(i + vec2<f32>(0.0, 1.0));
    let d = hash21(i + vec2<f32>(1.0, 1.0));

    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var value = 0.0;
    var amplitude = 0.5;
    var point = p;

    for (var octave = 0; octave < 4; octave = octave + 1) {
        value += amplitude * noise(point);
        point *= 2.03;
        amplitude *= 0.5;
    }

    return value;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = position.xy / max(u.resolution, vec2<f32>(1.0, 1.0));
    let aspect = u.resolution.x / max(u.resolution.y, 1.0);
    let p = vec2<f32>(uv.x * aspect, uv.y);
    let t = u.time * 0.045;

    let flow_a = fbm(p * 1.7 + vec2<f32>(t * 1.6, -t * 0.9));
    let flow_b = fbm(p * 2.6 + vec2<f32>(-t * 0.8, t * 0.5) + flow_a);
    let ribbon = smoothstep(0.28, 0.86, flow_b);
    let ribbon_ghost = smoothstep(0.42, 0.98, fbm(p * 3.3 + vec2<f32>(t * 0.6, flow_a * 1.4)));
    let veil = smoothstep(0.30, 1.0, flow_a * 0.7 + flow_b * 0.5);

    var color = u.abyss.rgb;
    color = mix(color, u.glow.rgb, ribbon * 0.82 * u.intensity);
    color = mix(color, u.accent.rgb, ribbon_ghost * 0.46 * u.intensity);
    color += u.glow.rgb * veil * 0.10 * u.intensity;

    let halo = exp(-4.5 * distance(uv, vec2<f32>(0.70, 0.24)));
    let pulse = 0.30 + 0.08 * sin(u.time * 0.9);
    color += u.highlight.rgb * halo * pulse * u.intensity;

    let vignette = smoothstep(0.20, 0.95, distance(uv, vec2<f32>(0.5, 0.5)));
    color *= 1.0 - vignette * 0.38;

    let grain = hash21(position.xy * 0.73 + u.time * 19.0) - 0.5;
    color += grain * 0.010;

    return vec4<f32>(color, 1.0);
}
