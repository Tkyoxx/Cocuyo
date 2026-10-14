use windows::core::w;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDeviceAndSwapChain, ID3D11Device, ID3D11DeviceContext, ID3D11RenderTargetView, ID3D11Texture2D,
    D3D11_CREATE_DEVICE_FLAG, D3D11_SDK_VERSION,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_MODE_DESC, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{IDXGISwapChain, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC, DXGI_USAGE_RENDER_TARGET_OUTPUT};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
    WS_VISIBLE,
};

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|arg| arg.parse().ok()).unwrap_or(20);
    unsafe {
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("Juego de prueba de Cocuyo"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            120,
            120,
            960,
            540,
            None,
            None,
            None,
            None,
        )
        .expect("ventana");
        let desc = DXGI_SWAP_CHAIN_DESC {
            BufferDesc: DXGI_MODE_DESC { Width: 944, Height: 501, Format: DXGI_FORMAT_R8G8B8A8_UNORM, ..Default::default() },
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
            BufferCount: 1,
            OutputWindow: window,
            Windowed: true.into(),
            ..Default::default()
        };
        let (mut chain, mut device, mut context): (Option<IDXGISwapChain>, Option<ID3D11Device>, Option<ID3D11DeviceContext>) =
            (None, None, None);
        D3D11CreateDeviceAndSwapChain(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&desc),
            Some(&mut chain),
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .expect("dispositivo");
        let (chain, device, context) = (chain.unwrap(), device.unwrap(), context.unwrap());
        let back: ID3D11Texture2D = chain.GetBuffer(0).expect("imagen");
        let mut view: Option<ID3D11RenderTargetView> = None;
        device.CreateRenderTargetView(&back, None, Some(&mut view)).expect("vista");
        let view = view.unwrap();

        let started = std::time::Instant::now();
        let mut message = MSG::default();
        while started.elapsed().as_secs() < seconds {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            let pulse = (started.elapsed().as_secs_f32() * 2.0).sin() * 0.1 + 0.25;
            context.ClearRenderTargetView(&view, &[0.05, pulse * 0.6, pulse, 1.0]);
            let _ = chain.Present(1, DXGI_PRESENT(0));
        }
    }
}
