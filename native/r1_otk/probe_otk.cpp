// Creo Object TOOLKIT (OTK) async probe — P0 gate for route R1.
//
// Two modes:
//   probe_otk start   "<creo cmd line>" "<text dir>"   // spawn a Creo session
//   probe_otk connect "<text dir>"                      // attach to an already
//                                                        // running Creo startet with
//                                                        // -g:no_graphics -i:rpc_input
//
// In both modes it then: creates a part, saves it, exports STEP.
//
// Build: build.bat in this folder (MSVC 2022 x64).

#include <ciplib.h>
#include <pfcAsyncConnection.h>
#include <pfcSession.h>
#include <pfcModel.h>
#include <pfcPart.h>
#include <pfcExceptions.h>
#include <pfcGlobal.h>

#include <iostream>
#include <string>

using namespace std;

static int run_work(pfcAsyncConnection_ptr conn, const std::string &mode)
{
    cout << "[probe] connected, is_running="
         << (conn->IsRunning() ? "true" : "false") << endl;

    pfcSession_ptr ses = conn->GetSession();
    if (!ses) {
        cout << "[probe] FAIL: no session" << endl;
        return 4;
    }

    cout << "[probe] creating part 'aicad_probe' ..." << endl;
    pfcPart_ptr part = ses->CreatePart("aicad_probe");
    if (!part) {
        cout << "[probe] FAIL: part not created" << endl;
        return 5;
    }
    cout << "[probe] part file = " << part->GetFileName() << endl;

    cout << "[probe] saving .prt ..." << endl;
    part->Save();
    cout << "[probe] saved" << endl;

    cout << "[probe] exporting STEP ..." << endl;
    pfcGeomExportFlags_ptr flags = pfcGeomExportFlags::Create();
    pfcExportInstructions_ptr step = pfcSTEPExportInstructions::Create(flags);
    part->Export("aicad_probe.step", step);
    cout << "[probe] STEP exported" << endl;

    if (mode == "connect") {
        cout << "[probe] disconnecting (Creo stays alive) ..." << endl;
        conn->Disconnect(30);
    } else {
        cout << "[probe] ending Creo session ..." << endl;
        conn->End();
    }
    cout << "[probe] done" << endl;
    return 0;
}

int main(int argc, char *argv[])
{
    if (argc < 3) {
        cout << "usage:" << endl;
        cout << "  probe_otk start   \"<creo cmd line>\" \"<text dir>\"" << endl;
        cout << "  probe_otk connect \"<text dir>\"" << endl;
        return 2;
    }

    const std::string mode = argv[1];
    int rc = 0;

    try {
        pfcAsyncConnection_ptr conn;

        if (mode == "connect") {
            const std::string textPath = argv[2];
            cout << "[probe] connecting to running Creo, text_path = "
                 << textPath << endl;
            conn = pfcAsyncConnection::Connect(NULL, NULL, textPath.c_str(), 120);
            if (!conn) {
                cout << "[probe] FAIL: could not attach to Creo" << endl;
                return 3;
            }
        } else if (mode == "start") {
            if (argc < 4) {
                cout << "[probe] start mode needs <creo cmd line> and <text dir>" << endl;
                return 2;
            }
            const std::string creoCmd = argv[2];
            const std::string textPath = argv[3];
            cout << "[probe] creo_cmd  = " << creoCmd << endl;
            cout << "[probe] text_path = " << textPath << endl;
            cout << "[probe] starting async connection ..." << endl;
            conn = pfcAsyncConnection::Start(creoCmd.c_str(), textPath.c_str());
            if (!conn) {
                cout << "[probe] FAIL: connection not established" << endl;
                return 3;
            }
        } else {
            cout << "[probe] unknown mode: " << mode << endl;
            return 2;
        }

        rc = run_work(conn, mode);
    }
    xcatchbegin
        xcatch (cipXException, ciperr)
        {
            cout << "[probe] FAIL: cip exception" << endl;
            rc = 6;
        }
        xcatch (pfcXPFC, xpfcerr)
        {
            cout << "[probe] FAIL: pfc exception -> " << xpfcerr << endl;
            rc = 7;
        }
        xcatch (pfcXToolkitError, e)
        {
            cout << "[probe] FAIL: toolkit error -> " << e << endl;
            rc = 8;
        }
    xcatchend
    catch (...) {
        cout << "[probe] FAIL: unknown exception" << endl;
        rc = 9;
    }

    return rc;
}
