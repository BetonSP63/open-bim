struct Camera { mvp: mat4x4<f32>, plane: vec4<f32>, depth: vec4<f32> }
@group(0) @binding(0) var<uniform> camera: Camera;
struct Vertex { @location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) c0: vec4<f32>, @location(3) c1: vec4<f32>, @location(4) c2: vec4<f32>, @location(5) c3: vec4<f32>, @location(6) color: vec4<f32> }
struct Output { @builtin(position) position: vec4<f32>, @location(0) world: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32> }
@vertex fn vs(v: Vertex) -> Output {
 let model = mat4x4<f32>(v.c0,v.c1,v.c2,v.c3); let world = model*vec4<f32>(v.position,1.0);
 var o: Output; o.position = camera.mvp*world; o.world=world.xyz; o.normal=(model*vec4<f32>(v.normal,0.0)).xyz; o.color=v.color; return o;
}
// Premultiplied output: opaque parts have alpha 1, see-through parts less.
@fragment fn fs(o: Output) -> @location(0) vec4<f32> {
 if dot(camera.plane,vec4<f32>(o.world,1.0))<0.0 || dot(camera.depth,vec4<f32>(o.world,1.0))<0.0 { discard; }
 let light=0.35+0.65*max(dot(normalize(o.normal),normalize(vec3<f32>(0.4,-0.6,0.8))),0.0);
 return vec4<f32>(o.color.rgb*light*o.color.a,o.color.a);
}
// Feature edges: a darker shade of the part colour, always opaque.
@fragment fn fs_edge(o: Output) -> @location(0) vec4<f32> {
 if dot(camera.plane,vec4<f32>(o.world,1.0))<0.0 || dot(camera.depth,vec4<f32>(o.world,1.0))<0.0 { discard; }
 return vec4<f32>(o.color.rgb*0.45,1.0);
}
