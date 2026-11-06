/* otk_bridge.cpp — OTK↔C API 混合模式连接桥（P1，ROADMAP 混合模式）
 *
 * 背景：C API ProEngineerConnect 连不上 `parametric -g:no_graphics -i:rpc_input`
 * 会话（rc=-4 PRO_TK_E_NOT_FOUND），而 R1 已验证 OTK
 * pfcAsyncConnection::Connect 能连上同一会话（native/r1_otk/README.md）。
 *
 * 本桥在同一进程内用 OTK 建立异步连接通道，之后 feat_build.c 继续用
 * C Toolkit 函数（ProSolidMdlnameCreate / ProFeatureWithoptionsCreate /
 * ProSolidRegenerate ...）完成特征构建 —— 即文档既定的混合模式。
 *
 * 导出（供 feat_build.c 以 extern "C" 调用）：
 *   int aicad_bridge_connect(const char *text_dir);
 *   int aicad_bridge_disconnect(int dwell_sec);
 */

#include <pfcAsyncConnection.h>
#include <pfcExceptions.h>
#include <ciplib.h>

#include <iostream>

static pfcAsyncConnection_ptr g_conn = NULL;

extern "C" int aicad_bridge_connect(const char *text_dir)
{
    try {
        if (g_conn)
            return 0; /* 已连接 */
        std::cout << "[bridge] OTK Connect(text=" << (text_dir ? text_dir : "")
                  << ", dwell=120) ..." << std::endl;
        g_conn = pfcAsyncConnection::Connect(NULL, NULL, text_dir, 120);
        if (!g_conn) {
            std::cerr << "[bridge] FAIL: Connect returned null" << std::endl;
            return -1;
        }
        std::cout << "[bridge] connected, is_running="
                  << (g_conn->IsRunning() ? "true" : "false") << std::endl;
        return 0;
    }
    xcatchbegin
        xcatch (pfcXToolkitError, e)
        {
            std::cerr << "[bridge] FAIL: XToolkitError -> " << e << std::endl;
            g_conn = NULL;
            return -2;
        }
        xcatch (pfcXPFC, e)
        {
            std::cerr << "[bridge] FAIL: XPFC -> " << e << std::endl;
            g_conn = NULL;
            return -3;
        }
        xcatch (cipXException, e)
        {
            std::cerr << "[bridge] FAIL: cip exception" << std::endl;
            g_conn = NULL;
            return -4;
        }
    xcatchend
    catch (...) {
        std::cerr << "[bridge] FAIL: unknown exception" << std::endl;
        g_conn = NULL;
        return -5;
    }
    /* 编译器无法从 xcatch 宏推断穷尽性；此返回不可达，仅为消除 C4715 */
    g_conn = NULL;
    return -6;
}

extern "C" int aicad_bridge_disconnect(int dwell_sec)
{
    try {
        if (g_conn) {
            g_conn->Disconnect(dwell_sec);
            g_conn = NULL;
            std::cout << "[bridge] disconnected (Creo stays alive)" << std::endl;
        }
        return 0;
    }
    catch (...) {
        g_conn = NULL;
        return -1;
    }
}
