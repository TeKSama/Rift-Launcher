// Rift Launcher : bouton Home et bouton capture des manettes, meme quand le jeu est au premier plan.
// made by TeK_Sama. Compile au demarrage par RiftLauncher.ps1 (Add-Type), en C# 5.
//
//  - Manettes Xbox : bouton Xbox lu par XInput (le bouton Partager est reserve a la Game Bar de Windows).
//  - DualShock 4, DualSense (PS5), Manette Pro Switch : boutons PS / Home et Share / Create / Capture
//    lus directement sur la manette (HID, en lecture partagee : le jeu continue de la recevoir).
using System;
using System.Collections.Generic;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Media;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using Microsoft.Win32.SafeHandles;

public static class RiftPad
{
    // ------------------------------------------------ reglages (ecrits par RiftLauncher.ps1)
    public static volatile bool HomeEnabled = true;
    public static volatile bool CaptureEnabled = true;
    public static volatile int GamePid = 0;
    public static volatile string GameName = "";
    public static volatile bool WantWindow = false;   // Home presse alors que la fenetre est fermee
    public static string CaptureRoot = "";
    public static string ShotExe = "";               // RiftShot.exe (capture DXGI), facultatif

    // ------------------------------------------------ etat lu par RiftLauncher.ps1
    public static int CaptureCount = 0;
    public static string LastCapture = "";
    static readonly List<string> logQueue = new List<string>();

    public static string[] TakeLogs()
    {
        lock (logQueue) { string[] a = logQueue.ToArray(); logQueue.Clear(); return a; }
    }

    static void Log(string m)
    {
        lock (logQueue) { if (logQueue.Count < 200) logQueue.Add(m); }
    }

    // ------------------------------------------------ Windows
    [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
    delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc f, IntPtr l);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern bool IsWindow(IntPtr h);
    [DllImport("user32.dll")] static extern bool IsIconic(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern bool BringWindowToTop(IntPtr h);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr h, int cmd);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a, uint b, bool attach);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);

    // ------------------------------------------------ XInput (bouton Xbox = 0x0400, seulement dans GetStateEx)
    [StructLayout(LayoutKind.Sequential)]
    struct XState { public uint Packet; public ushort Buttons; public byte LT; public byte RT; public short LX; public short LY; public short RX; public short RY; }
    [DllImport("xinput1_4.dll", EntryPoint = "#100")] static extern int XGetStateEx14(int i, out XState s);
    [DllImport("xinput1_3.dll", EntryPoint = "#100")] static extern int XGetStateEx13(int i, out XState s);

    // ------------------------------------------------ HID
    [StructLayout(LayoutKind.Sequential)]
    struct HIDD_ATTRIBUTES { public int Size; public ushort VendorID; public ushort ProductID; public ushort VersionNumber; }
    [StructLayout(LayoutKind.Sequential)]
    struct HIDP_CAPS
    {
        public ushort Usage; public ushort UsagePage;
        public ushort InputReportByteLength; public ushort OutputReportByteLength; public ushort FeatureReportByteLength;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 17)] public ushort[] Reserved;
        public ushort NumberLinkCollectionNodes;
        public ushort NumberInputButtonCaps; public ushort NumberInputValueCaps; public ushort NumberInputDataIndices;
        public ushort NumberOutputButtonCaps; public ushort NumberOutputValueCaps; public ushort NumberOutputDataIndices;
        public ushort NumberFeatureButtonCaps; public ushort NumberFeatureValueCaps; public ushort NumberFeatureDataIndices;
    }
    [StructLayout(LayoutKind.Sequential)]
    struct SP_DEVICE_INTERFACE_DATA { public int cbSize; public Guid InterfaceClassGuid; public int Flags; public IntPtr Reserved; }

    [DllImport("hid.dll")] static extern void HidD_GetHidGuid(out Guid g);
    [DllImport("hid.dll")] static extern bool HidD_GetAttributes(SafeFileHandle h, ref HIDD_ATTRIBUTES a);
    [DllImport("hid.dll")] static extern bool HidD_GetPreparsedData(SafeFileHandle h, out IntPtr p);
    [DllImport("hid.dll")] static extern bool HidD_FreePreparsedData(IntPtr p);
    [DllImport("hid.dll")] static extern int HidP_GetCaps(IntPtr p, out HIDP_CAPS c);
    [DllImport("setupapi.dll", CharSet = CharSet.Unicode)] static extern IntPtr SetupDiGetClassDevs(ref Guid g, IntPtr enumerator, IntPtr parent, int flags);
    [DllImport("setupapi.dll", SetLastError = true)] static extern bool SetupDiEnumDeviceInterfaces(IntPtr set, IntPtr devInfo, ref Guid g, int index, ref SP_DEVICE_INTERFACE_DATA d);
    [DllImport("setupapi.dll", CharSet = CharSet.Unicode, SetLastError = true)] static extern bool SetupDiGetDeviceInterfaceDetail(IntPtr set, ref SP_DEVICE_INTERFACE_DATA d, IntPtr detail, int size, out int required, IntPtr devInfo);
    [DllImport("setupapi.dll")] static extern bool SetupDiDestroyDeviceInfoList(IntPtr set);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern SafeFileHandle CreateFile(string name, uint access, uint share, IntPtr sec, uint disposition, uint flags, IntPtr template);

    enum Kind { None, DS4, DualSense, SwitchPro }

    static string NameOf(Kind k)
    {
        if (k == Kind.DS4) return "PlayStation 4";
        if (k == Kind.DualSense) return "PlayStation 5";
        if (k == Kind.SwitchPro) return "Switch Pro";
        return "?";
    }

    static Kind KindOf(ushort vid, ushort pid)
    {
        if (vid == 0x054C && (pid == 0x05C4 || pid == 0x09CC || pid == 0x0BA0)) return Kind.DS4;
        if (vid == 0x054C && (pid == 0x0CE6 || pid == 0x0DF2)) return Kind.DualSense;
        if (vid == 0x057E && pid == 0x2009) return Kind.SwitchPro;
        return Kind.None;
    }

    // ------------------------------------------------ demarrage
    static bool started;
    static readonly object gate = new object();
    static readonly HashSet<string> opened = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
    static readonly HashSet<string> ignored = new HashSet<string>(StringComparer.OrdinalIgnoreCase);

    public static void Start(string captureRoot)
    {
        if (started) return;
        started = true;
        CaptureRoot = captureRoot;
        try { SetProcessDPIAware(); } catch (Exception) { }   // captures en vraie resolution
        Thread x = new Thread(XLoop); x.IsBackground = true; x.Start();
        Thread s = new Thread(HidScan); s.IsBackground = true; s.Start();
    }

    // ------------------------------------------------ fenetres : launcher <-> jeu
    static IntPtr backTo = IntPtr.Zero;
    static DateTime lastToggle = DateTime.MinValue;
    static DateTime lastShot = DateTime.MinValue;

    public static IntPtr FindLauncher()
    {
        IntPtr found = IntPtr.Zero;
        EnumWindows(delegate (IntPtr h, IntPtr l)
        {
            if (!IsWindowVisible(h)) return true;
            StringBuilder t = new StringBuilder(256);
            GetWindowText(h, t, 256);
            if (t.ToString() != "Rift Launcher") return true;
            StringBuilder c = new StringBuilder(256);
            GetClassName(h, c, 256);
            if (!c.ToString().StartsWith("Chrome_WidgetWin")) return true;   // fenetre Edge, pas un dossier
            found = h;
            return false;
        }, IntPtr.Zero);
        return found;
    }

    static IntPtr GameWindow()
    {
        int pid = GamePid;
        if (pid == 0) return IntPtr.Zero;
        try
        {
            System.Diagnostics.Process p = System.Diagnostics.Process.GetProcessById(pid);
            p.Refresh();
            return p.MainWindowHandle;
        }
        catch (Exception) { return IntPtr.Zero; }
    }

    static void Focus(IntPtr h)
    {
        if (IsIconic(h)) ShowWindow(h, 9);   // SW_RESTORE
        IntPtr fg = GetForegroundWindow();
        uint pid;
        uint fgThread = GetWindowThreadProcessId(fg, out pid);
        uint me = GetCurrentThreadId();
        bool attached = fgThread != 0 && fgThread != me && AttachThreadInput(me, fgThread, true);
        BringWindowToTop(h);
        SetForegroundWindow(h);
        if (attached) AttachThreadInput(me, fgThread, false);
        if (GetForegroundWindow() != h)
        {
            // Windows refuse parfois : un appui sur Alt simule l'autorise.
            keybd_event(0x12, 0, 0, UIntPtr.Zero);
            keybd_event(0x12, 0, 2, UIntPtr.Zero);
            SetForegroundWindow(h);
        }
    }

    // Launcher devant -> retour au jeu ; sinon -> launcher devant.
    public static string Toggle()
    {
        lock (gate)
        {
            if ((DateTime.UtcNow - lastToggle).TotalMilliseconds < 450) return "ignore";   // meme appui vu deux fois
            lastToggle = DateTime.UtcNow;
            IntPtr launcher = FindLauncher();
            IntPtr fg = GetForegroundWindow();
            if (launcher == IntPtr.Zero)
            {
                if (GamePid != 0) { backTo = fg; WantWindow = true; return "ouverture"; }
                return "fenetre introuvable";
            }
            if (fg == launcher)
            {
                IntPtr target = GameWindow();
                if (target == IntPtr.Zero && backTo != IntPtr.Zero && IsWindow(backTo)) target = backTo;
                if (target == IntPtr.Zero) return "aucun jeu";
                Focus(target);
                return "jeu";
            }
            backTo = fg;
            Focus(launcher);
            return "launcher";
        }
    }

    // ------------------------------------------------ capture d'ecran
    static string SafeName(string s)
    {
        foreach (char c in Path.GetInvalidFileNameChars()) s = s.Replace(c, '_');
        s = s.Trim().TrimEnd('.');
        return s.Length == 0 ? "Rift Launcher" : s;
    }

    public static string Capture()
    {
        lock (gate)
        {
            if ((DateTime.UtcNow - lastShot).TotalMilliseconds < 700) return "";
            lastShot = DateTime.UtcNow;
        }
        try
        {
            IntPtr fg = GetForegroundWindow();
            Rectangle r = Screen.FromHandle(fg).Bounds;   // l'ecran ou se trouve le jeu
            string game = GameName;
            if (string.IsNullOrEmpty(game)) game = "Rift Launcher";
            string dir = Path.Combine(CaptureRoot, SafeName(game));
            Directory.CreateDirectory(dir);
            string stamp = DateTime.Now.ToString("yyyy-MM-dd HH-mm-ss");
            string file = Path.Combine(dir, stamp + ".png");
            for (int i = 2; File.Exists(file); i++) file = Path.Combine(dir, stamp + " (" + i + ").png");
            // 1) Capture fidele (DXGI) : la vraie image du jeu, meme en plein ecran sans bordure.
            string how = "classique";
            if (ShotDxgi(file, r.Left + r.Width / 2, r.Top + r.Height / 2)) how = "DXGI";
            else
            {
                // 2) Repli : capture classique de l'ecran.
                using (Bitmap bmp = new Bitmap(r.Width, r.Height, PixelFormat.Format24bppRgb))
                {
                    using (Graphics g = Graphics.FromImage(bmp))
                    {
                        g.CopyFromScreen(r.Left, r.Top, 0, 0, r.Size, CopyPixelOperation.SourceCopy);
                    }
                    bmp.Save(file, ImageFormat.Png);
                }
            }
            Shutter();
            Log("capture (" + how + ")");
            lock (gate) { CaptureCount++; LastCapture = file; }
            Log("capture enregistree : " + file);
            return file;
        }
        catch (Exception e)
        {
            Log("capture impossible : " + e.Message);
            return "";
        }
    }

    static bool dxgiBroken;
    static bool ShotDxgi(string file, int x, int y)
    {
        if (dxgiBroken || string.IsNullOrEmpty(ShotExe) || !File.Exists(ShotExe)) return false;
        try
        {
            System.Diagnostics.ProcessStartInfo psi = new System.Diagnostics.ProcessStartInfo();
            psi.FileName = ShotExe;
            psi.Arguments = "\"" + file + "\" " + x + " " + y;
            psi.UseShellExecute = false;
            psi.CreateNoWindow = true;
            psi.RedirectStandardError = true;
            using (System.Diagnostics.Process p = System.Diagnostics.Process.Start(psi))
            {
                string err = p.StandardError.ReadToEnd();
                if (!p.WaitForExit(8000)) { try { p.Kill(); } catch (Exception) { } Log("capture DXGI trop longue, capture classique"); return false; }
                if (p.ExitCode == 0 && File.Exists(file)) return true;
                Log("capture DXGI impossible (" + err.Trim() + "), capture classique");
                // Plantage dur (pas une simple erreur) : on n'insiste pas pendant cette session.
                if (p.ExitCode != 2) dxgiBroken = true;
                return false;
            }
        }
        catch (Exception e) { Log("capture DXGI impossible : " + e.Message); dxgiBroken = true; return false; }
    }

    // ------------------------------------------------ fenetres de l'Explorateur et boites de dialogue au premier plan
    // Windows empeche un programme en arriere-plan (notre serveur) de passer une fenetre devant :
    // on la repere puis on la met devant avec la meme methode que le bouton Home.
    public static IntPtr[] ExplorerWindows()
    {
        List<IntPtr> list = new List<IntPtr>();
        EnumWindows(delegate (IntPtr h, IntPtr l)
        {
            if (!IsWindowVisible(h)) return true;
            StringBuilder c = new StringBuilder(64);
            GetClassName(h, c, 64);
            if (c.ToString() == "CabinetWClass") list.Add(h);
            return true;
        }, IntPtr.Zero);
        return list.ToArray();
    }

    // Attend (4 s max) la fenetre de l'Explorateur qui vient de s'ouvrir, ou celle deja ouverte
    // sur ce dossier (titre = nom du dossier), et la met au premier plan.
    public static void FocusExplorerSoon(IntPtr[] before, string title)
    {
        HashSet<IntPtr> old = new HashSet<IntPtr>(before ?? new IntPtr[0]);
        Thread t = new Thread(delegate ()
        {
            for (int i = 0; i < 40; i++)
            {
                Thread.Sleep(100);
                IntPtr pick = IntPtr.Zero;
                foreach (IntPtr h in ExplorerWindows())
                {
                    if (!old.Contains(h)) { pick = h; break; }
                    if (!string.IsNullOrEmpty(title))
                    {
                        StringBuilder sb = new StringBuilder(512);
                        GetWindowText(h, sb, 512);
                        if (string.Equals(sb.ToString(), title, StringComparison.OrdinalIgnoreCase) && i >= 8) pick = h;
                    }
                }
                if (pick == IntPtr.Zero) continue;
                Thread.Sleep(120);   // laisse la fenetre finir de s'afficher
                try { lock (gate) { Focus(pick); } } catch (Exception) { }
                return;
            }
        });
        t.IsBackground = true;
        t.Start();
    }

    // Met une fenetre (par ex. la fenetre support d'une boite « Parcourir ») au premier plan.
    public static void ForceForeground(IntPtr h)
    {
        if (h == IntPtr.Zero) return;
        try { lock (gate) { Focus(h); } } catch (Exception) { }
    }

    // Plein ecran de la fenetre du launcher (F11) : demande par le mode Big Picture a la manette.
    public static string PressF11()
    {
        IntPtr launcher = FindLauncher();
        if (launcher == IntPtr.Zero) return "fenetre introuvable";
        if (GetForegroundWindow() != launcher) Focus(launcher);
        keybd_event(0x7A, 0, 0, UIntPtr.Zero);
        keybd_event(0x7A, 0, 2, UIntPtr.Zero);
        return "ok";
    }

    // Petit bruit d'obturateur synthetise (aucun fichier son).
    static byte[] shutterWav;
    static void Shutter()
    {
        try
        {
            if (shutterWav == null)
            {
                int rate = 22050, n = rate * 14 / 100;
                MemoryStream ms = new MemoryStream();
                BinaryWriter w = new BinaryWriter(ms);
                w.Write(Encoding.ASCII.GetBytes("RIFF")); w.Write(36 + n * 2); w.Write(Encoding.ASCII.GetBytes("WAVE"));
                w.Write(Encoding.ASCII.GetBytes("fmt ")); w.Write(16); w.Write((short)1); w.Write((short)1);
                w.Write(rate); w.Write(rate * 2); w.Write((short)2); w.Write((short)16);
                w.Write(Encoding.ASCII.GetBytes("data")); w.Write(n * 2);
                Random rnd = new Random(7);
                for (int i = 0; i < n; i++)
                {
                    double t = (double)i / rate;
                    double env = t < 0.05 ? Math.Exp(-t * 90) : 0.6 * Math.Exp(-(t - 0.05) * 70);   // deux « clic »
                    double v = (rnd.NextDouble() * 2 - 1) * env * 0.35;
                    w.Write((short)(v * short.MaxValue));
                }
                w.Flush();
                shutterWav = ms.ToArray();
            }
            SoundPlayer p = new SoundPlayer(new MemoryStream(shutterWav));
            p.Play();
        }
        catch (Exception) { }
    }

    static void Safe(Func<string> f)
    {
        try { f(); } catch (Exception e) { Log("erreur : " + e.Message); }
    }

    // ------------------------------------------------ boucle XInput (manettes Xbox)
    static void XLoop()
    {
        bool[] was = new bool[4];
        bool v14 = true;
        while (true)
        {
            bool any = false;
            for (int i = 0; i < 4; i++)
            {
                XState s = new XState();
                int r;
                try { r = v14 ? XGetStateEx14(i, out s) : XGetStateEx13(i, out s); }
                catch (Exception)
                {
                    if (v14) { v14 = false; i--; continue; }
                    Log("XInput indisponible : bouton Xbox desactive");
                    return;
                }
                if (r != 0) { was[i] = false; continue; }
                any = true;
                bool g = (s.Buttons & 0x0400) != 0;
                if (g && !was[i] && HomeEnabled) Safe(Toggle);
                was[i] = g;
            }
            Thread.Sleep(any ? 25 : 1500);
        }
    }

    // ------------------------------------------------ manettes PlayStation / Switch (HID)
    static List<string> HidPaths()
    {
        List<string> list = new List<string>();
        Guid g;
        HidD_GetHidGuid(out g);
        IntPtr set = SetupDiGetClassDevs(ref g, IntPtr.Zero, IntPtr.Zero, 0x12);   // PRESENT | DEVICEINTERFACE
        if (set == IntPtr.Zero || set == new IntPtr(-1)) return list;
        try
        {
            for (int i = 0; i < 512; i++)
            {
                SP_DEVICE_INTERFACE_DATA d = new SP_DEVICE_INTERFACE_DATA();
                d.cbSize = Marshal.SizeOf(typeof(SP_DEVICE_INTERFACE_DATA));
                if (!SetupDiEnumDeviceInterfaces(set, IntPtr.Zero, ref g, i, ref d)) break;
                int need;
                SetupDiGetDeviceInterfaceDetail(set, ref d, IntPtr.Zero, 0, out need, IntPtr.Zero);
                if (need <= 0) continue;
                IntPtr buf = Marshal.AllocHGlobal(need);
                try
                {
                    Marshal.WriteInt32(buf, IntPtr.Size == 8 ? 8 : 6);
                    if (SetupDiGetDeviceInterfaceDetail(set, ref d, buf, need, out need, IntPtr.Zero))
                        list.Add(Marshal.PtrToStringUni(new IntPtr(buf.ToInt64() + 4)));
                }
                finally { Marshal.FreeHGlobal(buf); }
            }
        }
        finally { SetupDiDestroyDeviceInfoList(set); }
        return list;
    }

    static Kind Probe(string path)
    {
        using (SafeFileHandle h = CreateFile(path, 0, 3, IntPtr.Zero, 3, 0, IntPtr.Zero))
        {
            if (h.IsInvalid) return Kind.None;
            HIDD_ATTRIBUTES a = new HIDD_ATTRIBUTES();
            a.Size = Marshal.SizeOf(typeof(HIDD_ATTRIBUTES));
            if (!HidD_GetAttributes(h, ref a)) return Kind.None;
            return KindOf(a.VendorID, a.ProductID);
        }
    }

    static void HidScan()
    {
        while (true)
        {
            try
            {
                foreach (string p in HidPaths())
                {
                    lock (opened) { if (opened.Contains(p) || ignored.Contains(p)) continue; }
                    Kind k = Probe(p);
                    if (k == Kind.None) { lock (opened) ignored.Add(p); continue; }
                    lock (opened) opened.Add(p);
                    Thread t = new Thread(Reader);
                    t.IsBackground = true;
                    t.Start(new object[] { p, k });
                }
            }
            catch (Exception e) { Log("recherche des manettes : " + e.Message); }
            Thread.Sleep(3000);
        }
    }

    static bool Bit(byte[] b, int n, int i, int bit)
    {
        return i < n && (b[i] & (1 << bit)) != 0;
    }

    // Lit les boutons Home et capture dans un rapport HID. len = taille des rapports (64 en USB).
    static bool Parse(Kind k, byte[] b, int n, int len, out bool home, out bool cap)
    {
        home = false; cap = false;
        if (n < 3) return false;
        byte id = b[0];
        if (k == Kind.DS4)
        {
            int o = id == 0x01 ? 0 : (id == 0x11 ? 2 : -1);   // USB / Bluetooth
            if (o < 0) return false;
            cap = Bit(b, n, 6 + o, 4);    // Share
            home = Bit(b, n, 7 + o, 0);   // PS
            return true;
        }
        if (k == Kind.DualSense)
        {
            if (id == 0x01 && len == 64) { cap = Bit(b, n, 9, 4); home = Bit(b, n, 10, 0); return true; }   // USB
            if (id == 0x31) { cap = Bit(b, n, 10, 4); home = Bit(b, n, 11, 0); return true; }               // Bluetooth complet
            if (id == 0x01) { cap = Bit(b, n, 6, 4); home = Bit(b, n, 7, 0); return true; }                // Bluetooth simple
            return false;
        }
        if (k == Kind.SwitchPro)
        {
            if (id == 0x30 || id == 0x21) { home = Bit(b, n, 4, 4); cap = Bit(b, n, 4, 5); return true; }   // mode complet
            if (id == 0x3F) { home = Bit(b, n, 2, 4); cap = Bit(b, n, 2, 5); return true; }                 // mode simple
            return false;
        }
        return false;
    }

    static void Reader(object arg)
    {
        object[] a = (object[])arg;
        string path = (string)a[0];
        Kind kind = (Kind)a[1];
        try
        {
            using (SafeFileHandle h = CreateFile(path, 0x80000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero))   // lecture, partagee
            {
                if (h.IsInvalid)
                {
                    Log("manette " + NameOf(kind) + " : lecture refusee (un autre programme la garde pour lui ?)");
                    lock (opened) ignored.Add(path);
                    return;
                }
                int len = 64;
                IntPtr pp;
                if (HidD_GetPreparsedData(h, out pp))
                {
                    HIDP_CAPS c;
                    if (HidP_GetCaps(pp, out c) == 0x00110000 && c.InputReportByteLength > 0) len = c.InputReportByteLength;
                    HidD_FreePreparsedData(pp);
                }
                Log("manette " + NameOf(kind) + " trouvee : boutons Home et capture actifs");
                using (FileStream fs = new FileStream(h, FileAccess.Read, 1, false))
                {
                    byte[] buf = new byte[len];
                    bool home = false, cap = false;
                    while (true)
                    {
                        int n = fs.Read(buf, 0, len);
                        if (n <= 0) break;
                        bool hNow, cNow;
                        if (!Parse(kind, buf, n, len, out hNow, out cNow)) continue;
                        if (hNow && !home && HomeEnabled) Safe(Toggle);
                        if (cNow && !cap && CaptureEnabled) Safe(Capture);
                        home = hNow;
                        cap = cNow;
                    }
                }
            }
            Log("manette " + NameOf(kind) + " deconnectee");
        }
        catch (Exception e) { Log("manette " + NameOf(kind) + " deconnectee (" + e.Message + ")"); }
        finally { lock (opened) opened.Remove(path); }
    }
}
