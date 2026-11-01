/*
 * Creo TOOLKIT (C API) feature-tree probe -- P1 gate for the R1/R2 mixed mode.
 *
 * P0 proved: headless Creo + OTK pfc async connect + CreatePart + Save.
 * P0 finding: the OTK pfc C++ API has no per-feature creation instructions
 *             (ProExtrude/pfcExtrudeInstructions do not exist there), so the
 *             feature tree must be built with the classic Creo TOOLKIT C API
 *             element tree (ProFeatureWithoptionsCreate).
 *
 * What this probe does (all headless, no ProSelect / no UI):
 *   1. ProEngineerConnect() to a Creo started with -g:no_graphics -i:rpc_input
 *   2. create a part                         (ProSolidMdlnameCreate)
 *   3. build the FIRST feature: sketch rectangle + extrude (blind depth)
 *   4. ProSolidRegenerate, ProMdlSave
 *   5. visit the feature tree and print every feature id/type
 *
 * Usage: probe_feat "<part name>" "<text dir>"
 * Build: build_feat.bat
 *
 * Sample source: <Creo>/Common Files/protoolkit/protk_appls/pt_userguide/
 *                ptu_featcreat/UgSktFirstFeatureCreate.c
 */

#include "ProToolkit.h"
#include "ProCore.h"
#include "ProMdl.h"
#include "ProSolid.h"
#include "ProModelitem.h"
#include "ProSelection.h"
#include "ProAsmcomppath.h"
#include "ProArray.h"
#include "ProElement.h"
#include "ProElempath.h"
#include "ProFeature.h"
#include "ProFeatType.h"
#include "ProFeatForm.h"
#include "ProExtrude.h"
#include "ProStdSection.h"
#include "ProSection.h"
#include "ProTKRunTime.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHK(call)                                                    \
    do {                                                             \
        ProError _e = (call);                                        \
        if (_e != PRO_TK_NO_ERROR) {                                 \
            printf("[feat] FAIL %-40s rc=%d\n", #call, (int)_e);      \
            return (int)_e;                                          \
        }                                                            \
        printf("[feat] ok   %s\n", #call);                           \
    } while (0)

/*---------------------------------------------------------------*\
  Build a 2-D rectangle section (the "sketcher" payload).
\*---------------------------------------------------------------*/
static int build_rect_section(ProSection section, double w, double h)
{
    Pro2dLinedef line;
    int line_id[4];
    ProWSecerror sec_errs;
    ProError e;

    e = ProSection2DAlloc(&section);
    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] FAIL ProSection2DAlloc rc=%d\n", (int)e);
        return (int)e;
    }

    line.type = PRO_2D_LINE;

    line.end1[0] = 0;    line.end1[1] = 0;
    line.end2[0] = w;    line.end2[1] = 0;
    CHK(ProSectionEntityAdd(section, (Pro2dEntdef *)&line, &line_id[0]));

    line.end1[0] = w;    line.end1[1] = 0;
    line.end2[0] = w;    line.end2[1] = h;
    CHK(ProSectionEntityAdd(section, (Pro2dEntdef *)&line, &line_id[1]));

    line.end1[0] = w;    line.end1[1] = h;
    line.end2[0] = 0;    line.end2[1] = h;
    CHK(ProSectionEntityAdd(section, (Pro2dEntdef *)&line, &line_id[2]));

    line.end1[0] = 0;    line.end1[1] = h;
    line.end2[0] = 0;    line.end2[1] = 0;
    CHK(ProSectionEntityAdd(section, (Pro2dEntdef *)&line, &line_id[3]));

    CHK(ProSectionEpsilonSet(section, 0.1));

    e = ProSecerrorAlloc(&sec_errs);
    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] FAIL ProSecerrorAlloc rc=%d\n", (int)e);
        return (int)e;
    }

    e = ProSectionAutodim(section, &sec_errs);
    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] FAIL ProSectionAutodim rc=%d\n", (int)e);
        ProSecerrorFree(&sec_errs);
        return (int)e;
    }

    e = ProSectionRegenerate(section, &sec_errs);
    ProSecerrorFree(&sec_errs);
    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] FAIL ProSectionRegenerate rc=%d\n", (int)e);
        return (int)e;
    }

    printf("[feat] ok   section built (%.0f x %.0f)\n", w, h);
    return 0;
}

/*---------------------------------------------------------------*\
  First feature: extruded protrusion from a rectangle.
\*---------------------------------------------------------------*/
static int create_first_extrude(ProSolid solid, double w, double h, double depth,
                                ProFeature *out_feat)
{
    ProError e, status;
    ProErrorlist errors;
    ProModelitem model_item;
    ProSelection model_sel;
    ProFeature feature;
    ProFeatureCreateOptions *opts = NULL;
    ProElempath path;
    ProElempathItem path_items[2];
    ProSection section;
    ProAsmcomppath comp_path;

    ProElement sketch_element;
    ProElement created_elemtree;
    ProElement tree, e_type, e_form;
    ProElement e_depth, e_from, e_from_type, e_from_val;
    ProElement e_to, e_to_type, e_to_val;

    (void)errors;

    /* root + first feature type ------------------------------- */
    CHK(ProElementAlloc(PRO_E_FEATURE_TREE, &tree));

    CHK(ProElementAlloc(PRO_E_FEATURE_TYPE, &e_type));
    CHK(ProElementIntegerSet(e_type, PRO_FEAT_FIRST_FEAT));
    CHK(ProElemtreeElementAdd(tree, NULL, e_type));

    /* form = extrude ------------------------------------------ */
    CHK(ProElementAlloc(PRO_E_FEATURE_FORM, &e_form));
    CHK(ProElementIntegerSet(e_form, PRO_EXTRUDE));
    CHK(ProElemtreeElementAdd(tree, NULL, e_form));

    /* extrude depth (blind, from + to) ------------------------- */
    CHK(ProElementAlloc(PRO_E_STD_EXT_DEPTH, &e_depth));
    CHK(ProElemtreeElementAdd(tree, NULL, e_depth));

    CHK(ProElementAlloc(PRO_E_EXT_DEPTH_FROM, &e_from));
    CHK(ProElemtreeElementAdd(e_depth, NULL, e_from));

    CHK(ProElementAlloc(PRO_E_EXT_DEPTH_FROM_TYPE, &e_from_type));
    CHK(ProElementIntegerSet(e_from_type, PRO_EXT_DEPTH_FROM_BLIND));
    CHK(ProElemtreeElementAdd(e_from, NULL, e_from_type));

    CHK(ProElementAlloc(PRO_E_EXT_DEPTH_FROM_VALUE, &e_from_val));
    CHK(ProElementDecimalsSet(e_from_val, 4));
    CHK(ProElementDoubleSet(e_from_val, depth));
    CHK(ProElemtreeElementAdd(e_from, NULL, e_from_val));

    CHK(ProElementAlloc(PRO_E_EXT_DEPTH_TO, &e_to));
    CHK(ProElemtreeElementAdd(e_depth, NULL, e_to));

    CHK(ProElementAlloc(PRO_E_EXT_DEPTH_TO_TYPE, &e_to_type));
    CHK(ProElementIntegerSet(e_to_type, PRO_EXT_DEPTH_TO_BLIND));
    CHK(ProElemtreeElementAdd(e_to, NULL, e_to_type));

    CHK(ProElementAlloc(PRO_E_EXT_DEPTH_TO_VALUE, &e_to_val));
    CHK(ProElementDecimalsSet(e_to_val, 4));
    CHK(ProElementDoubleSet(e_to_val, 0.0));
    CHK(ProElemtreeElementAdd(e_to, NULL, e_to_val));

    /* create the INCOMPLETE feature ---------------------------- */
    CHK(ProMdlToModelitem(solid, &model_item));
    CHK(ProSelectionAlloc(NULL, &model_item, &model_sel));

    e = ProArrayAlloc(1, sizeof(ProFeatureCreateOptions), 1,
                      (ProArray *)&opts);
    if (e != PRO_TK_NO_ERROR) return (int)e;
    opts[0] = PRO_FEAT_CR_INCOMPLETE_FEAT;

    status = ProFeatureWithoptionsCreate(model_sel, tree, opts,
                                         PRO_REGEN_NO_FLAGS, &feature, &errors);
    ProArrayFree((ProArray *)&opts);
    if (status != PRO_TK_NO_ERROR) {
        printf("[feat] FAIL ProFeatureWithoptionsCreate rc=%d\n", (int)status);
        ProElementFree(&tree);
        return (int)status;
    }
    printf("[feat] ok   incomplete feature created, id=%d\n", feature.id);

    /* pull the sketcher out and fill the section ---------------- */
    CHK(ProFeatureElemtreeExtract(&feature, NULL, PRO_FEAT_EXTRACT_NO_OPTS,
                                  &created_elemtree));

    path_items[0].type = PRO_ELEM_PATH_ITEM_TYPE_ID;
    path_items[0].path_item.elem_id = PRO_E_SKETCHER;
    CHK(ProElempathAlloc(&path));
    CHK(ProElempathDataSet(path, path_items, 1));
    CHK(ProElemtreeElementGet(created_elemtree, path, &sketch_element));

    memset(&section, 0, sizeof(section));
    CHK(ProElementSpecialvalueGet(sketch_element, NULL,
                                  (ProAppData *)&section));

    if (build_rect_section(section, w, h) != 0) {
        ProElempathFree(&path);
        ProFeatureElemtreeFree(&feature, created_elemtree);
        ProElementFree(&tree);
        return -1;
    }

    CHK(ProElementSpecialvalueSet(sketch_element, (ProAppData)section));
    ProElempathFree(&path);

    /* redefine to complete ------------------------------------- */
    CHK(ProSelectionAsmcomppathGet(model_sel, &comp_path));

    e = ProArrayAlloc(1, sizeof(ProFeatureCreateOptions), 1,
                      (ProArray *)&opts);
    if (e != PRO_TK_NO_ERROR) return (int)e;
    opts[0] = PRO_FEAT_CR_DEFINE_MISS_ELEMS;

    status = ProFeatureWithoptionsRedefine(&comp_path, &feature,
                                            created_elemtree, opts,
                                            PRO_REGEN_NO_FLAGS, &errors);
    ProArrayFree((ProArray *)&opts);

    ProFeatureElemtreeFree(&feature, created_elemtree);
    ProElementFree(&tree);

    if (status != PRO_TK_NO_ERROR) {
        printf("[feat] FAIL ProFeatureWithoptionsRedefine rc=%d\n", (int)status);
        return (int)status;
    }

    printf("[feat] ok   feature completed, id=%d type=%d\n",
           feature.id, (int)feature.type);
    *out_feat = feature;
    return 0;
}

/*---------------------------------------------------------------*\
  Feature tree dump.
\*---------------------------------------------------------------*/
static ProError visit_feature(ProFeature *p_feature, ProError status,
                              ProAppData app_data)
{
    int *count = (int *)app_data;
    if (status == PRO_TK_NO_ERROR) {
        (*count)++;
        printf("[tree] #%02d  id=%-4d type=%d\n", *count,
               p_feature->id, (int)p_feature->type);
    }
    return PRO_TK_NO_ERROR;
}

int main(int argc, char *argv[])
{
    ProError e;
    ProMdlName part_name;
    ProSolid solid;
    ProFeature feat;
    ProProcessHandle handle;
    ProBoolean random = PRO_B_FALSE;
    const char *name = "aicad_feat";
    const char *text = NULL;
    int count = 0;

    memset(&handle, 0, sizeof(handle));

    if (argc >= 2) name = argv[1];
    if (argc >= 3) text = argv[2];

    printf("[feat] part name = %s\n", name);
    printf("[feat] text dir  = %s\n", text ? text : "(none)");

    e = ProEngineerConnect((char *)"", (char *)"", (char *)"",
                           (char *)text, PRO_B_FALSE, 120, &random, &handle);
    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] connect #1 failed rc=%d, trying variants...\n", (int)e);

        e = ProEngineerConnect((char *)"", NULL, NULL, (char *)text,
                               PRO_B_TRUE, 120, &random, &handle);
        printf("[feat] connect #2 (random, null user/display) rc=%d\n", (int)e);

        if (e != PRO_TK_NO_ERROR) {
            e = ProEngineerConnect(NULL, NULL, NULL, (char *)text,
                                   PRO_B_TRUE, 120, &random, &handle);
            printf("[feat] connect #3 (null session id) rc=%d\n", (int)e);
        }

        if (e != PRO_TK_NO_ERROR) {
            e = ProEngineerConnect((char *)"", NULL, NULL, (char *)text,
                                   PRO_B_TRUE, 20, &random, &handle);
            printf("[feat] connect #4 (timeout 20) rc=%d\n", (int)e);
        }
    }

    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] FAIL ProEngineerConnect rc=%d\n", (int)e);
        return 3;
    }
    printf("[feat] connected\n");

    ProStringToWstring(part_name, (char *)name);
    e = ProSolidMdlnameCreate(part_name, PRO_MDLFILE_PART, &solid);
    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] FAIL ProSolidMdlnameCreate rc=%d\n", (int)e);
        ProEngineerDisconnect(&handle, 30);
        return 4;
    }
    printf("[feat] part created\n");

    if (create_first_extrude(solid, 100.0, 80.0, 40.0, &feat) != 0) {
        printf("[feat] FAIL: feature creation aborted\n");
        ProEngineerDisconnect(&handle, 30);
        return 5;
    }

    e = ProSolidRegenerate(solid, PRO_REGEN_NO_FLAGS);
    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] WARN ProSolidRegenerate rc=%d\n", (int)e);
    } else {
        printf("[feat] ok   ProSolidRegenerate\n");
    }

    memset(&count, 0, sizeof(count));
    e = ProSolidFeatVisit(solid, visit_feature, NULL, (ProAppData)&count);
    printf("[feat] feature tree: %d feature(s) visited (rc=%d)\n",
           count, (int)e);

    e = ProMdlSave(solid);
    if (e != PRO_TK_NO_ERROR) {
        printf("[feat] WARN ProMdlSave rc=%d\n", (int)e);
    } else {
        printf("[feat] ok   ProMdlSave -> %s.prt\n", name);
    }

    ProEngineerDisconnect(&handle, 30);
    printf("[feat] disconnected (Creo stays alive)\n");
    return 0;
}
