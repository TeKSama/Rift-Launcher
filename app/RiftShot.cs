// RiftShot.exe : capture d'ecran fidele pour Rift Launcher (DXGI Desktop Duplication).
// made by TeK_Sama. Compile par RiftLauncher.ps1 au premier besoin, en C# 5.
//
// Les jeux recents (Vulkan / DirectX en plein ecran sans bordure) s'affichent sans passer par
// la composition classique de Windows : une capture « GDI » renvoie alors une vieille image
// (celle du moment ou la fenetre a pris le focus). La duplication de bureau DXGI lit l'image
// reellement envoyee a l'ecran.
//
//   RiftShot.exe <fichier.png> <x> <y>     x, y : un point de l'ecran a capturer
//   code de sortie : 0 = ok, autre = echec (Rift Launcher fait alors une capture classique)
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;

static class RiftShot
{
    [DllImport("dxgi.dll")] static extern int CreateDXGIFactory1(ref Guid riid, out IntPtr factory);
    [DllImport("d3d11.dll")]
    static extern int D3D11CreateDevice(IntPtr adapter, int driverType, IntPtr software, uint flags, IntPtr levels,
                                        uint levelCount, uint sdkVersion, out IntPtr device, out int level, out IntPtr context);

    // Appels COM par la table des methodes (index verifies sur les en-tetes DXGI / D3D11).
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int EnumFn(IntPtr self, uint index, out IntPtr item);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int DescFn(IntPtr self, IntPtr desc);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate void VoidDescFn(IntPtr self, IntPtr desc);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int DuplicateFn(IntPtr self, IntPtr device, out IntPtr dup);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int AcquireFn(IntPtr self, uint timeoutMs, IntPtr info, out IntPtr resource);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int ReleaseFrameFn(IntPtr self);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int CreateTexFn(IntPtr self, IntPtr desc, IntPtr initial, out IntPtr texture);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate void CopyFn(IntPtr self, IntPtr dst, IntPtr src);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int MapFn(IntPtr self, IntPtr resource, uint sub, int type, uint flags, IntPtr mapped);
    [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate void UnmapFn(IntPtr self, IntPtr resource, uint sub);

    const int FACTORY1_ENUMADAPTERS1 = 12;   // IUnknown 3 + IDXGIObject 4 + IDXGIFactory 5
    const int ADAPTER_ENUMOUTPUTS = 7;
    const int OUTPUT_GETDESC = 7;
    const int OUTPUT1_DUPLICATEOUTPUT = 22;  // + IDXGIOutput 12 + 3 premieres de IDXGIOutput1
    const int DUP_GETDESC = 7;
    const int DUP_ACQUIRENEXTFRAME = 8;
    const int DUP_RELEASEFRAME = 14;
    const int DEVICE_CREATETEXTURE2D = 5;
    const int CONTEXT_MAP = 14;
    const int CONTEXT_UNMAP = 15;
    const int CONTEXT_COPYRESOURCE = 47;

    static readonly Guid IID_IDXGIFactory1 = new Guid("770aae78-f26f-4dba-a829-253c83d1b387");
    static readonly Guid IID_IDXGIOutput1 = new Guid("00cddea8-939b-4b83-a340-a685226666cc");
    static readonly Guid IID_ID3D11Texture2D = new Guid("6f15aaf2-d208-4e89-9ab4-489535d34f9c");
    const int DXGI_ERROR_NOT_FOUND = unchecked((int)0x887A0002);
    const int DXGI_ERROR_WAIT_TIMEOUT = unchecked((int)0x887A0027);

    static T Method<T>(IntPtr obj, int index) where T : class
    {
        IntPtr table = Marshal.ReadIntPtr(obj);
        IntPtr fn = Marshal.ReadIntPtr(table, index * IntPtr.Size);
        return (T)(object)Marshal.GetDelegateForFunctionPointer(fn, typeof(T));
    }

    static void Free(ref IntPtr p)
    {
        if (p != IntPtr.Zero) { Marshal.Release(p); p = IntPtr.Zero; }
    }

    static void Check(int hr, string what)
    {
        if (hr < 0) throw new Exception(what + " : 0x" + hr.ToString("X8"));
    }

    static int Main(string[] args)
    {
        if (args.Length < 3) { Console.Error.WriteLine("usage : RiftShot.exe fichier.png x y"); return 1; }
        try
        {
            Capture(args[0], int.Parse(args[1]), int.Parse(args[2]));
            return 0;
        }
        catch (Exception e)
        {
            Console.Error.WriteLine(e.Message);
            return 2;
        }
    }

    static void Capture(string file, int x, int y)
    {
        IntPtr factory = IntPtr.Zero, adapter = IntPtr.Zero, output = IntPtr.Zero, output1 = IntPtr.Zero;
        IntPtr device = IntPtr.Zero, context = IntPtr.Zero, dup = IntPtr.Zero, resource = IntPtr.Zero;
        IntPtr texture = IntPtr.Zero, staging = IntPtr.Zero;
        IntPtr buf = Marshal.AllocHGlobal(256);
        bool frameHeld = false;
        try
        {
            Guid iid = IID_IDXGIFactory1;
            Check(CreateDXGIFactory1(ref iid, out factory), "CreateDXGIFactory1");

            // L'ecran qui contient le point (x, y), sur la bonne carte graphique.
            bool found = false;
            for (uint a = 0; !found; a++)
            {
                int hr = Method<EnumFn>(factory, FACTORY1_ENUMADAPTERS1)(factory, a, out adapter);
                if (hr == DXGI_ERROR_NOT_FOUND) break;
                Check(hr, "EnumAdapters1");
                for (uint o = 0; ; o++)
                {
                    hr = Method<EnumFn>(adapter, ADAPTER_ENUMOUTPUTS)(adapter, o, out output);
                    if (hr == DXGI_ERROR_NOT_FOUND) break;
                    Check(hr, "EnumOutputs");
                    Check(Method<DescFn>(output, OUTPUT_GETDESC)(output, buf), "GetDesc");
                    int left = Marshal.ReadInt32(buf, 64), top = Marshal.ReadInt32(buf, 68);
                    int right = Marshal.ReadInt32(buf, 72), bottom = Marshal.ReadInt32(buf, 76);
                    if (x >= left && x < right && y >= top && y < bottom) { found = true; break; }
                    Free(ref output);
                }
                if (!found) Free(ref adapter);
            }
            if (!found) throw new Exception("ecran introuvable");

            int level;
            Check(D3D11CreateDevice(adapter, 0, IntPtr.Zero, 0x20, IntPtr.Zero, 0, 7, out device, out level, out context), "D3D11CreateDevice");
            Guid iidOut1 = IID_IDXGIOutput1;
            Check(Marshal.QueryInterface(output, ref iidOut1, out output1), "IDXGIOutput1");
            Check(Method<DuplicateFn>(output1, OUTPUT1_DUPLICATEOUTPUT)(output1, device, out dup), "DuplicateOutput");

            Method<VoidDescFn>(dup, DUP_GETDESC)(dup, buf);
            int width = Marshal.ReadInt32(buf, 0), height = Marshal.ReadInt32(buf, 4), format = Marshal.ReadInt32(buf, 16);
            if (format != 87 && format != 91) throw new Exception("format d'ecran non gere (" + format + ", HDR ?)");

            // Premiere image disponible (la duplication donne tout de suite l'image affichee).
            for (int i = 0; i < 6 && resource == IntPtr.Zero; i++)
            {
                int hr = Method<AcquireFn>(dup, DUP_ACQUIRENEXTFRAME)(dup, 250, buf, out resource);
                if (hr == DXGI_ERROR_WAIT_TIMEOUT) continue;
                Check(hr, "AcquireNextFrame");
                frameHeld = true;
                if (resource == IntPtr.Zero) { Method<ReleaseFrameFn>(dup, DUP_RELEASEFRAME)(dup); frameHeld = false; }
            }
            if (resource == IntPtr.Zero) throw new Exception("aucune image recue");

            Guid iidTex = IID_ID3D11Texture2D;
            Check(Marshal.QueryInterface(resource, ref iidTex, out texture), "ID3D11Texture2D");

            // Texture lisible par le processeur, de la meme taille.
            int[] desc = { width, height, 1, 1, format, 1, 0, 3 /* STAGING */, 0, 0x20000 /* CPU_ACCESS_READ */, 0 };
            Marshal.Copy(desc, 0, buf, desc.Length);
            Check(Method<CreateTexFn>(device, DEVICE_CREATETEXTURE2D)(device, buf, IntPtr.Zero, out staging), "CreateTexture2D");
            Method<CopyFn>(context, CONTEXT_COPYRESOURCE)(context, staging, texture);
            Check(Method<MapFn>(context, CONTEXT_MAP)(context, staging, 0, 1 /* READ */, 0, buf), "Map");
            try
            {
                IntPtr data = Marshal.ReadIntPtr(buf, 0);
                int pitch = Marshal.ReadInt32(buf, IntPtr.Size);
                using (Bitmap bmp = new Bitmap(width, height, PixelFormat.Format32bppRgb))
                {
                    BitmapData bd = bmp.LockBits(new Rectangle(0, 0, width, height), ImageLockMode.WriteOnly, PixelFormat.Format32bppRgb);
                    byte[] row = new byte[width * 4];
                    for (int r = 0; r < height; r++)
                    {
                        Marshal.Copy(new IntPtr(data.ToInt64() + (long)r * pitch), row, 0, row.Length);
                        Marshal.Copy(row, 0, new IntPtr(bd.Scan0.ToInt64() + (long)r * bd.Stride), row.Length);
                    }
                    bmp.UnlockBits(bd);
                    bmp.Save(file, ImageFormat.Png);
                }
            }
            finally { Method<UnmapFn>(context, CONTEXT_UNMAP)(context, staging, 0); }
        }
        finally
        {
            if (frameHeld && dup != IntPtr.Zero) Method<ReleaseFrameFn>(dup, DUP_RELEASEFRAME)(dup);
            Free(ref staging); Free(ref texture); Free(ref resource); Free(ref dup);
            Free(ref output1); Free(ref context); Free(ref device); Free(ref output); Free(ref adapter); Free(ref factory);
            Marshal.FreeHGlobal(buf);
        }
    }
}
