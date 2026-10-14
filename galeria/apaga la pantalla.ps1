# Apaga el monitor; vuelve al mover el mouse.
Add-Type -Namespace Cocuyo -Name Pantalla -MemberDefinition '[DllImport("user32.dll")] public static extern int SendMessage(int hWnd, int msg, int wParam, int lParam);'
[void][Cocuyo.Pantalla]::SendMessage(0xFFFF, 0x0112, 0xF170, 2)
"Pantalla apagada."
