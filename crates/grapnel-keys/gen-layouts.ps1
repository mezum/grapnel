# Prints the symbol rows of src/layouts.rs from the keyboard layouts installed with Windows.
# Run: pwsh crates/grapnel-keys/gen-layouts.ps1
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class Kbd {
    [DllImport("user32.dll")] static extern IntPtr LoadKeyboardLayout(string id, uint flags);
    [DllImport("user32.dll")] static extern bool UnloadKeyboardLayout(IntPtr hkl);
    [DllImport("user32.dll")] static extern uint MapVirtualKeyEx(uint code, uint type, IntPtr hkl);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int ToUnicodeEx(uint vk, uint sc, byte[] state, StringBuilder buf, int len, uint flags, IntPtr hkl);
    static string Char(uint vk, bool shift, IntPtr hkl) {
        var state = new byte[256];
        if (shift) state[0x10] = 0x80;
        var buf = new StringBuilder(8);
        int n = ToUnicodeEx(vk, MapVirtualKeyEx(vk, 0, hkl), state, buf, 8, 4, hkl);
        string s = n == 0 ? "" : buf.ToString(0, 1);
        // A dead key stays pending; press it again to clear the state.
        if (n < 0) ToUnicodeEx(vk, MapVirtualKeyEx(vk, 0, hkl), state, buf, 8, 4, hkl);
        return s;
    }
    public static string Rows(string id) {
        IntPtr hkl = LoadKeyboardLayout(id, 0x80); // KLF_NOTELLSHELL
        var sb = new StringBuilder();
        var vks = new uint[] { 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
            0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xDB, 0xDC, 0xDD, 0xDE, 0xDF, 0xE2 };
        foreach (uint vk in vks) {
            if (MapVirtualKeyEx(vk, 0, hkl) == 0) continue;
            string plain = Char(vk, false, hkl), shifted = Char(vk, true, hkl);
            if (plain.Length == 1 && char.IsLetterOrDigit(plain[0]) && plain[0] < 128) plain = "";
            if (plain != "" && shifted == plain.ToUpperInvariant()) shifted = "";
            if (shifted.Length == 1 && char.IsDigit(shifted[0])) shifted = "";
            if (plain == "" && shifted == "") continue;
            string name = vk < 0x40 ? "b'" + (char)vk + "'" : string.Format("0x{0:X2}", vk);
            sb.AppendFormat("    ({0}, {1}, {2}),\n", name, Lit(plain), Lit(shifted));
        }
        UnloadKeyboardLayout(hkl);
        return sb.ToString();
    }
    static string Lit(string s) { return "\"" + s.Replace("\\", "\\\\").Replace("\"", "\\\"") + "\""; }
}
'@
[Console]::OutputEncoding = [Text.Encoding]::UTF8
foreach ($l in @(@('JIS', '00000411'), @('US', '00000409'), @('UK', '00000809'), @('DE', '00000407'), @('FR', '0000040C'))) {
    "const $($l[0]): &[Row] = &["
    [Kbd]::Rows($l[1]).TrimEnd("`n")
    "];"
}
