// el cursor encima de la grabacion. se compila con: fxc /T vs_4_0 /E vs /O3 /Qstrip_reflect /Qstrip_debug /Fo cursor_vs.cso cursor.hlsl (y ps_4_0 /E ps para cursor_ps.cso)
struct V { float4 p : SV_Position; float2 t : TEXCOORD0; };
V vs(uint i : SV_VertexID) {
    V o;
    float2 t = float2(i & 1, i >> 1);
    o.p = float4(t.x * 2 - 1, 1 - t.y * 2, 0, 1);
    o.t = t;
    return o;
}
Texture2D tex : register(t0);
SamplerState pick : register(s0);
float4 ps(V i) : SV_Target { return tex.Sample(pick, i.t); }
