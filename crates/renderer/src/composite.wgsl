@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;
struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vs(@builtin(vertex_index) i:u32) -> Output {
 let positions=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
 let p=positions[i]; var o:Output; o.position=vec4<f32>(p,0.0,1.0); o.uv=vec2<f32>((p.x+1.0)*0.5,(1.0-p.y)*0.5); return o;
}
@fragment fn fs(o:Output)->@location(0) vec4<f32> {return textureSample(image,image_sampler,o.uv);}
