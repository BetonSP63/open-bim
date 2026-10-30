struct Camera { mvp: mat4x4<f32>, plane: vec4<f32>, depth: vec4<f32>, edge: vec4<f32> }
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

// Screen-expanded edge quads retain the ordinary world clipping and depth test.
struct EdgeVertex { @location(0) a: vec3<f32>, @location(1) b: vec3<f32>, @location(7) corner: vec2<f32>, @location(2) c0: vec4<f32>, @location(3) c1: vec4<f32>, @location(4) c2: vec4<f32>, @location(5) c3: vec4<f32>, @location(6) color: vec4<f32> }
struct EdgeOutput { @builtin(position) position: vec4<f32>, @location(0) world: vec3<f32>, @location(1) color: vec4<f32>, @location(2) @interpolate(linear) along: f32 }
@vertex fn vs_edge(v: EdgeVertex) -> EdgeOutput {
 let model = mat4x4<f32>(v.c0,v.c1,v.c2,v.c3);
 var a = model*vec4<f32>(v.a,1.0); var b = model*vec4<f32>(v.b,1.0);
 var ca = camera.mvp*a; var cb = camera.mvp*b;
 // Clip segments crossing the eye plane before perspective division.
 if ca.w < 0.0001 && cb.w >= 0.0001 { let t=(0.0001-ca.w)/(cb.w-ca.w); a=mix(a,b,t); ca=camera.mvp*a; }
 if cb.w < 0.0001 && ca.w >= 0.0001 { let t=(0.0001-cb.w)/(ca.w-cb.w); b=mix(b,a,t); cb=camera.mvp*b; }
 var o: EdgeOutput;
 let da = ca.xy/max(ca.w,0.0001); let db = cb.xy/max(cb.w,0.0001);
 let delta = (db-da)*camera.edge.xy*0.5;
 let length = max(length(delta),0.0001);
 let perpendicular = vec2<f32>(-delta.y,delta.x)/length;
 var pos = mix(ca,cb,v.corner.x);
 pos = vec4<f32>(pos.xy + perpendicular*v.corner.y*max(camera.edge.z,0.5)/camera.edge.xy*pos.w, pos.z, pos.w);
 // Tiny reversed-Z offset prevents coplanar edges disappearing in shaded views.
 pos.z = min(pos.w, pos.z + 0.000001*pos.w);
 o.position=pos; o.world=mix(a,b,v.corner.x).xyz; o.color=v.color; o.along=v.corner.x*length;
 return o;
}
@fragment fn fs_wide_edge(o: EdgeOutput) -> @location(0) vec4<f32> {
 if dot(camera.plane,vec4<f32>(o.world,1.0))<0.0 || dot(camera.depth,vec4<f32>(o.world,1.0))<0.0 { discard; }
 if camera.edge.w > 1.5 { let p=o.along%19.0; if (p>12.0 && p<15.0) || p>16.0 { discard; } }
 else if camera.edge.w > 0.5 && o.along%9.0 > 6.0 { discard; }
 return vec4<f32>(o.color.rgb*0.45,1.0);
}
