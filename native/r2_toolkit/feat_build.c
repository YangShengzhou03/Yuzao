/*
 * feat_build.c -- FeatureSpec 驱动的 Creo 特征树构建器（R2 C Toolkit，headless）。
 *
 * 对应 ROADMAP 任务：P1.2（sketch + extrude）、P1.3（hole）。
 * 契约：spec/featurespec.schema.json + spec/rust（ADR-012：唯一写入内核的输入）。
 *
 * 与探针的差异：probe_feat.c 是硬编码参数的最小探针（P1 门禁）；
 * 本程序把 FeatureSpec JSON 作为唯一输入，产出真实特征树 .prt（G1/G3/G4）。
 *
 * 实现依据（本机 Creo 12.4.0.0 官方资料，非凭记忆）：
 *   - 首特征 sketch+extrude 元素树：protk_appls/pt_userguide/ptu_featcreat/
 *     UgSktFirstFeatureCreate.c（probe_feat.c 已编译验证同一结构）
 *   - 直孔元素树：ptu_featcreat/UgHoleCreate.c
 *     （PRO_E_HLE_COM / PRO_E_HLE_PLACEMENT，放置参考改为程序化解析，
 *      官方示例用 ProSelect 需要 UI，headless 不可用）
 *   - 曲面遍历/平面几何：ProSolidSurfaceVisit + ProSurfaceDataGet（ProSurface.h）
 *   - datum 平面按名查找：ProModelitemByNameInit + ProFeatureTypeGet==PRO_FEAT_DATUM
 *
 * v1 能力边界（诚实声明，非隐藏缺陷）：
 *   - 仅 part（assembly 在 P3.1 / ROADMAP）；仅 features[0]=sketch + features[1]=extrude
 *     的首特征对（后续拉伸在 P1.4 覆盖度验证后扩展）；
 *   - hole 仅 face[top]/face[bottom]，通孔（IR v1 无孔深字段）；线性放置参考固定取
 *     模板 datum RIGHT/FRONT，孔心取放置面 xyz 包围盒中心（P2 用 UV 精确求值替代）；
 *   - pattern 以多实例独立直孔执行（每个都是真实 hole 特征，G1 成立）；
 *     原生阵列特征在 P3.2 / ROADMAP 落地；
 *   - edits 由 Rust 调度层在 P2 编辑通道执行，本构建器只回显不执行。
 *
 * 用法：feat_build <spec.json> [text_dir]
 * 退出码：0 成功；3 连接失败；5 特征创建失败；7 再生失败；8 特征树校验不一致；
 *         9 保存失败；2 输入/校验错误。
 * 输出协议：行前缀 [aicad]（信息）/ [aicad:map]（特征 id 映射）/ [aicad:error]（错误）。
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
#include "ProBodyOpts.h"
#include "ProExtrude.h"
#include "ProStdSection.h"
#include "ProSection.h"
#include "ProHole.h"
#include "ProDtmCsys.h"
#include "ProDtmPln.h"
#include "ProSurfacedata.h"
#include "ProGeomitemdata.h"
#include "ProTKRunTime.h"
#include "ProUtil.h"

#include <math.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* ==================================================================== *\
   结构化错误输出
\* ==================================================================== */

static void aicad_info(const char *fmt, ...)
{
    va_list ap;
    printf("[aicad] ");
    va_start(ap, fmt);
    vprintf(fmt, ap);
    va_end(ap);
    printf("\n");
}

static void aicad_map(const char *fmt, ...)
{
    va_list ap;
    printf("[aicad:map] ");
    va_start(ap, fmt);
    vprintf(fmt, ap);
    va_end(ap);
    printf("\n");
}

static void aicad_error(const char *code, const char *fmt, ...)
{
    va_list ap;
    printf("[aicad:error code=%s] ", code);
    va_start(ap, fmt);
    vprintf(fmt, ap);
    va_end(ap);
    printf("\n");
}

/* ProErrorlist 是裸结构（ProItemerr.h:25）：直接读，无需访问器。
   特征创建/重定义失败时 p_errors 里有 Creo 给的真实原因，必须打出来。 */
static void dump_pro_errors(ProErrorlist *errors)
{
    if (!errors || errors->error_number <= 0) {
        aicad_info("pro_errors: （空）");
        return;
    }
    aicad_info("pro_errors: 共 %d 条", errors->error_number);
    for (int i = 0; i < errors->error_number; i++) {
        aicad_info("pro_errors[%d] elem_id=%d type=%d error=%d",
                   i, errors->error_list[i].err_item_id,
                   (int)errors->error_list[i].err_item_type,
                   (int)errors->error_list[i].error);
    }
}

/* ==================================================================== *\
   极简严格 JSON 解析器（FeatureSpec 子集，无第三方依赖）
   支持：object/array/string(转义)/number/true/false/null；
   拒绝：尾随逗号、重复键、非法转义、超深/超量节点。
\* ==================================================================== */

#define J_MAX_NODES 2048
#define J_MAX_DEPTH 32
#define J_MAX_STR   256

typedef enum { J_NULL, J_BOOL, J_NUM, J_STR, J_ARR, J_OBJ } JKind;

typedef struct JVal {
    JKind kind;
    double num;                 /* J_NUM / J_BOOL(0|1) */
    char str[J_MAX_STR];        /* J_STR / object key */
    int child[64];              /* J_ARR/J_OBJ: 子节点索引（OBJ 交替 key,value） */
    int nchild;
} JVal;

typedef struct {
    JVal *nodes;                /* 堆分配（整体约 1 MB，避免占满线程栈） */
    int count;
    const char *src;
    size_t pos, len;
    int depth;
} JParser;

static int j_alloc(JParser *p, JKind k, int *out)
{
    if (p->count >= J_MAX_NODES) { aicad_error("FS201", "JSON 节点数超过上限 %d", J_MAX_NODES); return -1; }
    memset(&p->nodes[p->count], 0, sizeof(JVal));
    p->nodes[p->count].kind = k;
    *out = p->count++;
    return 0;
}

static void j_skipws(JParser *p)
{
    while (p->pos < p->len) {
        char c = p->src[p->pos];
        if (c == ' ' || c == '\t' || c == '\r' || c == '\n') p->pos++;
        else break;
    }
}

static int j_value(JParser *p, int *out);

static int j_string_raw(JParser *p, char *buf, size_t bufsz)
{
    /* 前提：p->src[p->pos] == '"' */
    p->pos++;
    size_t n = 0;
    while (p->pos < p->len) {
        char c = p->src[p->pos++];
        if (c == '"') { buf[n] = 0; return 0; }
        if (c == '\\') {
            if (p->pos >= p->len) break;
            char e = p->src[p->pos++];
            switch (e) {
            case '"':  c = '"';  break;
            case '\\': c = '\\'; break;
            case '/':  c = '/';  break;
            case 'b':  c = '\b'; break;
            case 'f':  c = '\f'; break;
            case 'n':  c = '\n'; break;
            case 'r':  c = '\r'; break;
            case 't':  c = '\t'; break;
            case 'u': {
                if (p->pos + 4 > p->len) { aicad_error("FS201", "\\u 转义不完整（offset %zu）", p->pos); return -1; }
                unsigned v = 0;
                for (int i = 0; i < 4; i++) {
                    char h = p->src[p->pos++];
                    v <<= 4;
                    if (h >= '0' && h <= '9') v |= (unsigned)(h - '0');
                    else if (h >= 'a' && h <= 'f') v |= (unsigned)(h - 'a' + 10);
                    else if (h >= 'A' && h <= 'F') v |= (unsigned)(h - 'A' + 10);
                    else { aicad_error("FS201", "\\u 含非十六进制字符（offset %zu）", p->pos); return -1; }
                }
                if (v > 127) { aicad_error("FS201", "FeatureSpec 仅允许 ASCII 字符（\\u%04x）", v); return -1; }
                c = (char)v;
                break;
            }
            default:
                aicad_error("FS201", "非法转义 \\%c（offset %zu）", e, p->pos - 1);
                return -1;
            }
        }
        if (n + 1 >= bufsz) { aicad_error("FS201", "字符串超长（上限 %d）", J_MAX_STR); return -1; }
        buf[n++] = c;
    }
    aicad_error("FS201", "字符串未闭合");
    return -1;
}

static int j_object(JParser *p, int idx)
{
    JVal *v = &p->nodes[idx];
    p->pos++; /* { */
    j_skipws(p);
    if (p->pos < p->len && p->src[p->pos] == '}') { p->pos++; return 0; }
    for (;;) {
        j_skipws(p);
        if (p->pos >= p->len || p->src[p->pos] != '"') {
            aicad_error("FS201", "对象键应为字符串（offset %zu）", p->pos);
            return -1;
        }
        int key, val;
        if (j_alloc(p, J_STR, &key) != 0) return -1;
        if (j_string_raw(p, p->nodes[key].str, J_MAX_STR) != 0) return -1;
        j_skipws(p);
        if (p->pos >= p->len || p->src[p->pos] != ':') {
            aicad_error("FS201", "对象键后缺少 ':'（offset %zu）", p->pos);
            return -1;
        }
        p->pos++;
        if (j_value(p, &val) != 0) return -1;
        /* 重复键检测 */
        for (int i = 0; i + 1 < v->nchild; i += 2) {
            if (strcmp(p->nodes[v->child[i]].str, p->nodes[key].str) == 0) {
                aicad_error("FS201", "重复的 JSON 键：%s", p->nodes[key].str);
                return -1;
            }
        }
        if (v->nchild + 2 > (int)(sizeof(v->child) / sizeof(v->child[0]))) {
            aicad_error("FS201", "对象成员数超限");
            return -1;
        }
        v->child[v->nchild++] = key;
        v->child[v->nchild++] = val;
        j_skipws(p);
        if (p->pos < p->len && p->src[p->pos] == ',') { p->pos++; continue; }
        if (p->pos < p->len && p->src[p->pos] == '}') { p->pos++; return 0; }
        aicad_error("FS201", "对象应为 ',' 或 '}'（offset %zu）", p->pos);
        return -1;
    }
}

static int j_array(JParser *p, int idx)
{
    JVal *v = &p->nodes[idx];
    p->pos++; /* [ */
    j_skipws(p);
    if (p->pos < p->len && p->src[p->pos] == ']') { p->pos++; return 0; }
    for (;;) {
        int c;
        if (j_value(p, &c) != 0) return -1;
        if (v->nchild + 1 > (int)(sizeof(v->child) / sizeof(v->child[0]))) {
            aicad_error("FS201", "数组长度超限");
            return -1;
        }
        v->child[v->nchild++] = c;
        j_skipws(p);
        if (p->pos < p->len && p->src[p->pos] == ',') { p->pos++; continue; }
        if (p->pos < p->len && p->src[p->pos] == ']') { p->pos++; return 0; }
        aicad_error("FS201", "数组应为 ',' 或 ']'（offset %zu）", p->pos);
        return -1;
    }
}

static int j_value(JParser *p, int *out)
{
    if (++p->depth > J_MAX_DEPTH) { aicad_error("FS201", "JSON 嵌套超过 %d 层", J_MAX_DEPTH); return -1; }
    j_skipws(p);
    if (p->pos >= p->len) { aicad_error("FS201", "JSON 提前结束"); return -1; }
    char c = p->src[p->pos];
    int idx;
    if (c == '{') {
        if (j_alloc(p, J_OBJ, &idx) != 0) return -1;
        if (j_object(p, idx) != 0) return -1;
    } else if (c == '[') {
        if (j_alloc(p, J_ARR, &idx) != 0) return -1;
        if (j_array(p, idx) != 0) return -1;
    } else if (c == '"') {
        if (j_alloc(p, J_STR, &idx) != 0) return -1;
        if (j_string_raw(p, p->nodes[idx].str, J_MAX_STR) != 0) return -1;
    } else if (c == 't' && p->len - p->pos >= 4 && strncmp(p->src + p->pos, "true", 4) == 0) {
        if (j_alloc(p, J_BOOL, &idx) != 0) return -1;
        p->nodes[idx].num = 1.0;
        p->pos += 4;
    } else if (c == 'f' && p->len - p->pos >= 5 && strncmp(p->src + p->pos, "false", 5) == 0) {
        if (j_alloc(p, J_BOOL, &idx) != 0) return -1;
        p->nodes[idx].num = 0.0;
        p->pos += 5;
    } else if (c == 'n' && p->len - p->pos >= 4 && strncmp(p->src + p->pos, "null", 4) == 0) {
        if (j_alloc(p, J_NULL, &idx) != 0) return -1;
        p->pos += 4;
    } else if (c == '-' || (c >= '0' && c <= '9')) {
        char *end = NULL;
        double d = strtod(p->src + p->pos, &end);
        if (end == p->src + p->pos) { aicad_error("FS201", "非法数字（offset %zu）", p->pos); return -1; }
        if (j_alloc(p, J_NUM, &idx) != 0) return -1;
        p->nodes[idx].num = d;
        p->pos = (size_t)(end - p->src);
    } else {
        aicad_error("FS201", "非法字符 '%c'（offset %zu）", c, p->pos);
        return -1;
    }
    p->depth--;
    *out = idx;
    return 0;
}

/* 解析入口：校验尾随内容，返回根节点索引或 -1 */
static int j_parse(const char *text, JParser *p)
{
    memset(p, 0, sizeof(*p));
    p->nodes = (JVal *)calloc(J_MAX_NODES, sizeof(JVal));
    if (!p->nodes) { aicad_error("FS200", "JSON 解析器内存分配失败"); return -1; }
    p->src = text;
    p->len = strlen(text);
    int root;
    if (j_value(p, &root) != 0) return -1;
    j_skipws(p);
    if (p->pos != p->len) {
        aicad_error("FS201", "JSON 后存在尾随内容（offset %zu）", p->pos);
        return -1;
    }
    return root;
}

/* --- 便捷访问 --- */
static JVal *j_obj_get(JParser *p, int obj, const char *key)
{
    JVal *v = &p->nodes[obj];
    if (v->kind != J_OBJ) return NULL;
    for (int i = 0; i + 1 < v->nchild; i += 2)
        if (strcmp(p->nodes[v->child[i]].str, key) == 0)
            return &p->nodes[v->child[i + 1]];
    return NULL;
}

/* ==================================================================== *\
   FeatureSpec 的 C 侧模型（校验规则与 spec/rust 对齐的子集）
\* ==================================================================== */

typedef struct {
    char id[J_MAX_STR];
    char plane[J_MAX_STR];
    double w, h;
} CSketch;

typedef struct {
    char id[J_MAX_STR];
    char sketch_ref[J_MAX_STR];
    double depth;
    int symmetric;
} CExtrude;

typedef struct {
    char id[J_MAX_STR];
    char on_feature[J_MAX_STR];
    char on_face[J_MAX_STR];    /* 空 = 未指定面 */
    double dia;
    int pat_n[2];
    double pat_pitch[2];
    int has_pattern;
} CHole;

typedef struct {
    char model[J_MAX_STR];
    CSketch sketch;
    CExtrude extrude;
    CHole holes[32];
    int n_holes;
    int n_edits;
    int has_sketch, has_extrude;
} CSpec;

static int is_feature_id(const char *s)
{
    size_t n = strlen(s);
    if (n < 4 || s[0] != 'F') return 0;
    for (size_t i = 1; i < n; i++)
        if (s[i] < '0' || s[i] > '9') return 0;
    return 1;
}

static int is_model_name(const char *s)
{
    size_t n = strlen(s);
    if (n == 0 || n > 32) return 0;
    if (!(s[0] >= 'A' && s[0] <= 'Z') && !(s[0] >= 'a' && s[0] <= 'z')) return 0;
    for (size_t i = 0; i < n; i++) {
        char c = s[i];
        if (!((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
              (c >= '0' && c <= '9') || c == '_')) return 0;
    }
    return 1;
}

/* 解析拓扑引用 F002 / F002.face[top] */
static int parse_topo_ref(const char *s, char *fid, char *face)
{
    face[0] = 0;
    const char *m = strstr(s, ".face[");
    if (m) {
        size_t fl = strlen(m);
        if (fl < 7 || m[fl - 1] != ']') return -1;
        size_t flen = fl - 6 - 1;
        if (flen == 0 || flen >= J_MAX_STR) return -1;
        memcpy(face, m + 6, flen);
        face[flen] = 0;
        for (const char *c = face; *c; c++)
            if (!((*c >= 'a' && *c <= 'z') || (*c >= '0' && *c <= '9') || *c == '_'))
                return -1;
        if ((size_t)(m - s) >= J_MAX_STR) return -1;
        memcpy(fid, s, (size_t)(m - s));
        fid[m - s] = 0;
    } else {
        if (strlen(s) >= J_MAX_STR) return -1;
        strcpy(fid, s);
    }
    return is_feature_id(fid) ? 0 : -1;
}

#define SPEC_CHECK(cond, code, ...) \
    do { if (!(cond)) { aicad_error(code, __VA_ARGS__); return -1; } } while (0)

/* 把解析后的 JSON 映射为 CSpec；返回 0 成功，-1 已输出结构化错误 */
static int spec_from_json(JParser *p, int root, CSpec *spec)
{
    memset(spec, 0, sizeof(*spec));
    JVal *v = &p->nodes[root];
    SPEC_CHECK(v->kind == J_OBJ, "FS201", "FeatureSpec 根必须是对象");

    JVal *m = j_obj_get(p, root, "model");
    JVal *t = j_obj_get(p, root, "type");
    JVal *fs = j_obj_get(p, root, "features");
    SPEC_CHECK(m && m->kind == J_STR, "FS201", "缺少 model（string）");
    SPEC_CHECK(t && t->kind == J_STR, "FS201", "缺少 type（string）");
    SPEC_CHECK(fs && fs->kind == J_ARR, "FS201", "缺少 features（array）");
    SPEC_CHECK(is_model_name(m->str), "FS101", "model 名称不合法：%s", m->str);
    SPEC_CHECK(strcmp(t->str, "part") == 0, "FS211",
               "v1 执行器仅支持 type=part（assembly 在 P3.1 落地），得到：%s", t->str);
    strncpy(spec->model, m->str, J_MAX_STR - 1);

    /* ---- features ---- */
    SPEC_CHECK(fs->nchild >= 1, "FS201", "features 不能为空");
    SPEC_CHECK(fs->nchild <= 16, "FS202", "v1 执行器最多 16 个特征节点，得到 %d", fs->nchild);

    for (int i = 0; i < fs->nchild; i++) {
        int fi = fs->child[i];
        JVal *f = &p->nodes[fi];
        SPEC_CHECK(f->kind == J_OBJ, "FS201", "features[%d] 必须是对象", i);
        JVal *id = j_obj_get(p, fi, "id");
        JVal *op = j_obj_get(p, fi, "op");
        SPEC_CHECK(id && id->kind == J_STR, "FS201", "features[%d] 缺少 id", i);
        SPEC_CHECK(op && op->kind == J_STR, "FS201", "features[%d] 缺少 op", i);
        SPEC_CHECK(is_feature_id(id->str), "FS110", "特征 id 不合法：%s", id->str);

        if (strcmp(op->str, "sketch") == 0) {
            SPEC_CHECK(i == 0, "FS202", "v1 执行器要求 features[0] 为 sketch（实际 features[%d]）", i);
            SPEC_CHECK(!spec->has_sketch, "FS111", "重复的 sketch 特征：%s", id->str);
            JVal *pl = j_obj_get(p, fi, "plane");
            JVal *pr = j_obj_get(p, fi, "profile");
            JVal *dm = j_obj_get(p, fi, "dims");
            SPEC_CHECK(pl && pl->kind == J_STR, "FS120", "sketch %s 缺少 plane", id->str);
            SPEC_CHECK(pr && pr->kind == J_STR, "FS120", "sketch %s 缺少 profile", id->str);
            SPEC_CHECK(dm && dm->kind == J_OBJ, "FS120", "sketch %s 缺少 dims", id->str);
            const char *ok_planes[] = { "TOP", "FRONT", "RIGHT", "BOTTOM", "BACK", "LEFT" };
            int ok = 0;
            for (size_t k = 0; k < sizeof(ok_planes) / sizeof(ok_planes[0]); k++)
                if (strcmp(pl->str, ok_planes[k]) == 0) ok = 1;
            SPEC_CHECK(ok, "FS133", "未知基准面：%s", pl->str);
            SPEC_CHECK(strcmp(pr->str, "rect") == 0, "FS170", "v1 仅支持 profile=rect，得到：%s", pr->str);
            JVal *w = j_obj_get(p, (int)(dm - p->nodes), "w");
            JVal *h = j_obj_get(p, (int)(dm - p->nodes), "h");
            SPEC_CHECK(w && w->kind == J_NUM, "FS120", "sketch %s 缺少 dims.w", id->str);
            SPEC_CHECK(h && h->kind == J_NUM, "FS120", "sketch %s 缺少 dims.h", id->str);
            SPEC_CHECK(w->num > 0 && h->num > 0, "FS140", "轮廓尺寸必须 > 0（w=%g, h=%g）", w->num, h->num);
            strncpy(spec->sketch.id, id->str, J_MAX_STR - 1);
            strncpy(spec->sketch.plane, pl->str, J_MAX_STR - 1);
            spec->sketch.w = w->num;
            spec->sketch.h = h->num;
            spec->has_sketch = 1;
        } else if (strcmp(op->str, "extrude") == 0) {
            SPEC_CHECK(i == 1 && spec->has_sketch, "FS202",
                       "v1 执行器要求 features[1] 为引用 %s 的 extrude（实际 features[%d]）",
                       spec->sketch.id, i);
            SPEC_CHECK(!spec->has_extrude, "FS111", "重复的 extrude 特征：%s", id->str);
            JVal *sk = j_obj_get(p, fi, "sketch");
            JVal *dp = j_obj_get(p, fi, "depth");
            JVal *sd = j_obj_get(p, fi, "side");
            SPEC_CHECK(sk && sk->kind == J_STR, "FS120", "extrude %s 缺少 sketch", id->str);
            SPEC_CHECK(dp && dp->kind == J_NUM, "FS120", "extrude %s 缺少 depth", id->str);
            SPEC_CHECK(sd && sd->kind == J_STR, "FS120", "extrude %s 缺少 side", id->str);
            SPEC_CHECK(strcmp(sk->str, spec->sketch.id) == 0, "FS202",
                       "v1 首特征对的 extrude.sketch 必须为 %s，得到：%s", spec->sketch.id, sk->str);
            SPEC_CHECK(dp->num > 0, "FS140", "拉伸深度必须 > 0（得到 %g）", dp->num);
            SPEC_CHECK(strcmp(sd->str, "symmetric") == 0 || strcmp(sd->str, "one_side") == 0,
                       "FS133", "未知 side：%s", sd->str);
            strncpy(spec->extrude.id, id->str, J_MAX_STR - 1);
            strncpy(spec->extrude.sketch_ref, sk->str, J_MAX_STR - 1);
            spec->extrude.depth = dp->num;
            spec->extrude.symmetric = (strcmp(sd->str, "symmetric") == 0);
            spec->has_extrude = 1;
        } else if (strcmp(op->str, "hole") == 0) {
            SPEC_CHECK(spec->has_extrude, "FS202", "hole %s 出现在首特征对之前", id->str);
            SPEC_CHECK(spec->n_holes < (int)(sizeof(spec->holes) / sizeof(spec->holes[0])),
                       "FS202", "hole 特征数超过 v1 上限 32");
            CHole *h = &spec->holes[spec->n_holes];
            memset(h, 0, sizeof(*h));
            strncpy(h->id, id->str, J_MAX_STR - 1);
            JVal *on = j_obj_get(p, fi, "on");
            JVal *dia = j_obj_get(p, fi, "dia");
            SPEC_CHECK(on && on->kind == J_STR, "FS120", "hole %s 缺少 on", id->str);
            SPEC_CHECK(dia && dia->kind == J_NUM, "FS120", "hole %s 缺少 dia", id->str);
            char fid[J_MAX_STR];
            SPEC_CHECK(parse_topo_ref(on->str, fid, h->on_face) == 0, "FS150",
                       "拓扑引用语法不合法：%s", on->str);
            SPEC_CHECK(strcmp(fid, spec->extrude.id) == 0, "FS202",
                       "v1 执行器 hole.on 仅支持 %s（首特征拉伸体），得到：%s",
                       spec->extrude.id, fid);
            if (h->on_face[0]) {
                SPEC_CHECK(strcmp(h->on_face, "top") == 0 || strcmp(h->on_face, "bottom") == 0,
                           "FS203", "v1 执行器仅支持 face[top]/face[bottom]，得到：face[%s]", h->on_face);
            }
            SPEC_CHECK(dia->num > 0, "FS140", "孔径必须 > 0（得到 %g）", dia->num);
            h->dia = dia->num;
            JVal *pat = j_obj_get(p, fi, "pattern");
            if (pat) {
                int pi = (int)(pat - p->nodes);
                JVal *pt = j_obj_get(p, pi, "type");
                JVal *pn = j_obj_get(p, pi, "n");
                JVal *pp = j_obj_get(p, pi, "pitch");
                SPEC_CHECK(pt && pt->kind == J_STR && strcmp(pt->str, "rect") == 0,
                           "FS142", "v1 仅支持 pattern.type=rect，得到：%s", pt ? pt->str : "(null)");
                SPEC_CHECK(pn && pn->kind == J_ARR && pn->nchild == 2, "FS141", "pattern.n 必须是二元数组");
                SPEC_CHECK(pp && pp->kind == J_ARR && pp->nchild == 2, "FS140", "pattern.pitch 必须是二元数组");
                for (int k = 0; k < 2; k++) {
                    JVal *nk = &p->nodes[pn->child[k]];
                    JVal *pk = &p->nodes[pp->child[k]];
                    SPEC_CHECK(nk->kind == J_NUM && nk->num >= 1 && nk->num == floor(nk->num),
                               "FS141", "阵列实例数必须为 >=1 的整数（方向 %d 得到 %g）", k, nk->num);
                    SPEC_CHECK(pk->kind == J_NUM && pk->num > 0, "FS140",
                               "阵列间距必须 > 0（方向 %d 得到 %g）", k, pk->num);
                    h->pat_n[k] = (int)nk->num;
                    h->pat_pitch[k] = pk->num;
                }
                SPEC_CHECK(h->pat_n[0] * h->pat_n[1] <= 100, "FS210",
                           "阵列实例总数超过上限 100（%d x %d）", h->pat_n[0], h->pat_n[1]);
                h->has_pattern = 1;
            }
            spec->n_holes++;
        } else {
            aicad_error("FS212", "未知 op：%s（允许 sketch / extrude / hole）", op->str);
            return -1;
        }
    }

    SPEC_CHECK(spec->has_sketch && spec->has_extrude, "FS202",
               "v1 执行器要求 features[0]=sketch 且 features[1]=extrude 成对出现");

    /* ---- edits：v1 由 Rust 调度层在 P2 编辑通道执行，这里仅回显 ---- */
    JVal *edits = j_obj_get(p, root, "edits");
    if (edits) {
        SPEC_CHECK(edits->kind == J_ARR, "FS201", "edits 必须是数组");
        spec->n_edits = edits->nchild;
        for (int i = 0; i < edits->nchild; i++) {
            int ei = edits->child[i];
            JVal *tg = j_obj_get(p, ei, "target");
            JVal *vl = j_obj_get(p, ei, "value");
            if (tg && vl && tg->kind == J_STR)
                aicad_info("edit[%d] target=%s value=%g（P2 编辑通道执行，本构建器跳过）",
                           i, tg->str, vl->kind == J_NUM ? vl->num : 0.0);
        }
    }
    return 0;
}

/* ==================================================================== *\
   Creo 操作
\* ==================================================================== */

#define CHK(call)                                                          \
    do {                                                                   \
        ProError _e = (call);                                              \
        if (_e != PRO_TK_NO_ERROR) {                                       \
            aicad_error("FS206", "%s 失败 rc=%d", #call, (int)_e);          \
            return (int)_e;                                                \
        }                                                                  \
    } while (0)

/*---- 矩形截面（沿用 probe_feat.c 已验证实现） ----*/
/* 注意：ProSection2DAlloc 会新建 section，必须传 ProSection* 才能把新句柄交回调用方。
   按值传 ProSection 时，函数内 ProSection2DAlloc(&section) 写的是局部副本，
   调用方拿到的仍是旧句柄 —— 本次排查的真实 bug。 */
static int build_rect_section(ProSection *p_section, double w, double h)
{
    Pro2dLinedef line;
    int line_id[4];
    ProWSecerror sec_errs;
    ProError e;
    ProSection section;

    if (*p_section == NULL) {
        e = ProSection2DAlloc(p_section);
        if (e != PRO_TK_NO_ERROR) { aicad_error("FS206", "ProSection2DAlloc rc=%d", (int)e); return (int)e; }
    }
    section = *p_section;

    line.type = PRO_2D_LINE;
    line.end1[0] = 0; line.end1[1] = 0; line.end2[0] = w; line.end2[1] = 0;
    CHK(ProSectionEntityAdd(section, (Pro2dEntdef *)&line, &line_id[0]));
    line.end1[0] = w; line.end1[1] = 0; line.end2[0] = w; line.end2[1] = h;
    CHK(ProSectionEntityAdd(section, (Pro2dEntdef *)&line, &line_id[1]));
    line.end1[0] = w; line.end1[1] = h; line.end2[0] = 0; line.end2[1] = h;
    CHK(ProSectionEntityAdd(section, (Pro2dEntdef *)&line, &line_id[2]));
    line.end1[0] = 0; line.end1[1] = h; line.end2[0] = 0; line.end2[1] = 0;
    CHK(ProSectionEntityAdd(section, (Pro2dEntdef *)&line, &line_id[3]));

    CHK(ProSectionEpsilonSet(section, 0.1));
    CHK(ProSecerrorAlloc(&sec_errs));
    e = ProSectionAutodim(section, &sec_errs);
    if (e != PRO_TK_NO_ERROR) { ProSecerrorFree(&sec_errs); aicad_error("FS206", "ProSectionAutodim rc=%d", (int)e); return (int)e; }
    e = ProSectionRegenerate(section, &sec_errs);
    ProSecerrorFree(&sec_errs);
    if (e != PRO_TK_NO_ERROR) { aicad_error("FS206", "ProSectionRegenerate rc=%d", (int)e); return (int)e; }
    return 0;
}

/*---- datum 平面按名查找（定义在后，此处前置声明供首特征复用） ----*/
static int find_datum_plane(ProSolid solid, const char *name, ProModelitem *out);

/*---- 首特征：sketch + extrude ----
   权威流程 = TestExtrude.c ProTestFirstFeatureCreate + ProTestFeatureSectionInit
   （incomplete create → extract → sketcher 塞 section → DEFINE_MISS_ELEMS redefine）：
   1. 输入树极简：FEATURE_TYPE/FIRST_FEAT、FEATURE_FORM/PRO_EXTRUDE、
      STD_EXT_DEPTH→FROM→FROM_VALUE。没有 section、没有深度 TYPE、没有
      MATRLSIDE/DIRECTION —— Creo 用默认值补齐。实测教训：在输入树里显式写
      FROM_BLIND+TO_SYMMETRIC（违反 ProExtrude.h Note 2）会让 redefine 挂起
      （headless 下 Creo 弹交互 dashboard，无人应答，永久阻塞）。
   2. 对称拉伸：按 Note 2 加 FROM_TYPE=FROM_NONE + TO{TYPE=SYMMETRIC,VALUE=depth}，
      官方最小树不支持对称，这是最小扩展。 */
static int create_first_extrude(ProSolid solid, const CSpec *spec, ProFeature *out_feat)
{
    ProError e, status;
    ProErrorlist errors;
    ProModelitem model_item;
    ProSelection model_sel;
    ProFeature feature;
    ProFeatureCreateOptions *opts = NULL;
    ProSection section = NULL;

    ProElement tree, e_type, e_form, e_depth, e_from, e_from_val;
    ProElement created_tree = NULL, e_sketcher = NULL;
    ProElempathItem path_items[1];
    ProElempath path = NULL;

    double depth = spec->extrude.depth;

    /* 1) 输入树（官方 ProTestFirstFeatureCreate 极简版） */
    CHK(ProElementAlloc(PRO_E_FEATURE_TREE, &tree));
    CHK(ProElementAlloc(PRO_E_FEATURE_TYPE, &e_type));
    CHK(ProElementIntegerSet(e_type, PRO_FEAT_FIRST_FEAT));
    CHK(ProElemtreeElementAdd(tree, NULL, e_type));
    CHK(ProElementAlloc(PRO_E_FEATURE_FORM, &e_form));
    CHK(ProElementIntegerSet(e_form, PRO_EXTRUDE));
    CHK(ProElemtreeElementAdd(tree, NULL, e_form));
    CHK(ProElementAlloc(PRO_E_STD_EXT_DEPTH, &e_depth));
    CHK(ProElemtreeElementAdd(tree, NULL, e_depth));
    CHK(ProElementAlloc(PRO_E_EXT_DEPTH_FROM, &e_from));
    CHK(ProElemtreeElementAdd(e_depth, NULL, e_from));
    CHK(ProElementAlloc(PRO_E_EXT_DEPTH_FROM_VALUE, &e_from_val));
    CHK(ProElementDecimalsSet(e_from_val, 4));
    CHK(ProElementDoubleSet(e_from_val, depth));
    CHK(ProElemtreeElementAdd(e_from, NULL, e_from_val));

    if (spec->extrude.symmetric) {
        /* Note 2：对称 = FROM_NONE + TO{SYMMETRIC, depth}（官方最小树的唯一扩展） */
        ProElement e_ft, e_to, e_tt, e_tv;
        CHK(ProElementAlloc(PRO_E_EXT_DEPTH_FROM_TYPE, &e_ft));
        CHK(ProElementIntegerSet(e_ft, PRO_EXT_DEPTH_FROM_NONE));
        CHK(ProElemtreeElementAdd(e_from, NULL, e_ft));
        CHK(ProElementAlloc(PRO_E_EXT_DEPTH_TO, &e_to));
        CHK(ProElemtreeElementAdd(e_depth, NULL, e_to));
        CHK(ProElementAlloc(PRO_E_EXT_DEPTH_TO_TYPE, &e_tt));
        CHK(ProElementIntegerSet(e_tt, PRO_EXT_DEPTH_SYMMETRIC));
        CHK(ProElemtreeElementAdd(e_to, NULL, e_tt));
        CHK(ProElementAlloc(PRO_E_EXT_DEPTH_TO_VALUE, &e_tv));
        CHK(ProElementDecimalsSet(e_tv, 4));
        CHK(ProElementDoubleSet(e_tv, depth));
        CHK(ProElemtreeElementAdd(e_to, NULL, e_tv));
    }

    CHK(ProMdlToModelitem((ProMdl)solid, &model_item));
    CHK(ProSelectionAlloc(NULL, &model_item, &model_sel));

    e = ProArrayAlloc(1, sizeof(ProFeatureCreateOptions), 1, (ProArray *)&opts);
    if (e != PRO_TK_NO_ERROR) { ProElementFree(&tree); return (int)e; }
    opts[0] = PRO_FEAT_CR_INCOMPLETE_FEAT;

    memset(&errors, 0, sizeof(errors));
    status = ProFeatureWithoptionsCreate(model_sel, tree, opts, PRO_REGEN_NO_FLAGS,
                                         &feature, &errors);
    ProArrayFree((ProArray *)&opts);
    ProElementFree(&tree);
    if (status != PRO_TK_NO_ERROR) {
        aicad_error("FS206", "ProFeatureWithoptionsCreate（首特征 incomplete）rc=%d", (int)status);
        dump_pro_errors(&errors);
        return (int)status;
    }
    aicad_info("incomplete 首特征已创建 id=%d", feature.id);

    /* 2) extract → sketcher → section（官方 ProTestFeatureSectionInit is_first 分支：
       首特征的 PRO_E_SKETCHER 在 extracted tree 根级，1 项路径） */
    CHK(ProFeatureElemtreeExtract(&feature, NULL, PRO_FEAT_EXTRACT_NO_OPTS, &created_tree));
    path_items[0].type = PRO_ELEM_PATH_ITEM_TYPE_ID;
    path_items[0].path_item.elem_id = PRO_E_SKETCHER;
    CHK(ProElempathAlloc(&path));
    CHK(ProElempathDataSet(path, path_items, 1));
    CHK(ProElemtreeElementGet(created_tree, path, &e_sketcher));
    {
        ProSection cur = NULL;
        ProError ge = ProElementSpecialvalueGet(e_sketcher, NULL, (ProAppData *)&cur);
        aicad_info("sketcher 现有 section: Get rc=%d", (int)ge);
        /* build_rect_section 在 *p_section==NULL 时自建，否则往现有 section 里画矩形 */
        section = (ge == PRO_TK_NO_ERROR) ? cur : NULL;
        if (build_rect_section(&section, spec->sketch.w, spec->sketch.h) != 0) {
            ProElempathFree(&path);
            ProFeatureElemtreeFree(&feature, created_tree);
            return -1;
        }
        CHK(ProElementSpecialvalueSet(e_sketcher, (ProAppData)section));
    }
    ProElempathFree(&path);
    path = NULL;

    /* 3) redefine（官方：DEFINE_MISS_ELEMS + Part 模式 NULL comp_path，单次尝试） */
    e = ProArrayAlloc(1, sizeof(ProFeatureCreateOptions), 1, (ProArray *)&opts);
    if (e != PRO_TK_NO_ERROR) { ProFeatureElemtreeFree(&feature, created_tree); return (int)e; }
    opts[0] = PRO_FEAT_CR_DEFINE_MISS_ELEMS;
    memset(&errors, 0, sizeof(errors));
    status = ProFeatureWithoptionsRedefine(NULL, &feature, created_tree, opts,
                                           PRO_REGEN_NO_FLAGS, &errors);
    ProArrayFree((ProArray *)&opts);
    ProFeatureElemtreeFree(&feature, created_tree);
    if (status != PRO_TK_NO_ERROR) {
        aicad_error("FS206", "ProFeatureWithoptionsRedefine（首特征）rc=%d", (int)status);
        dump_pro_errors(&errors);
        return (int)status;
    }

    aicad_map("%s(sketch+%s) -> ProFeature id=%d", spec->sketch.id, spec->extrude.id, feature.id);
    *out_feat = feature;
    return 0;
}

/*---- 放置面查找：按 face 名选平面（top=最高 z 平面，bottom=最低 z 平面） ----*/
typedef struct {
    ProSolid solid;
    const char *face;       /* "top" / "bottom" */
    int found;
    int best_id;
    double best_z;
    double center[2];       /* 放置面中心 xy */
} FaceCtx;

static ProError face_filter(ProSurface s, ProAppData data)
{
    (void)data;
    ProGeomitemdata *gd = NULL;
    if (ProSurfaceDataGet(s, &gd) != PRO_TK_NO_ERROR || !gd ||
        gd->obj_type != PRO_SURFACE || (PRO_SURF_DATA(gd)) == NULL)
        return PRO_TK_CONTINUE;
    ProSurfacedata *sd = PRO_SURF_DATA(gd);
    ProError rc = (sd->type == PRO_SRF_PLANE) ? PRO_TK_NO_ERROR : PRO_TK_CONTINUE;
    ProGeomitemdataFree(&gd);
    return rc;
}

static ProError face_action(ProSurface s, ProError status, ProAppData data)
{
    (void)status;
    FaceCtx *ctx = (FaceCtx *)data;
    ProGeomitemdata *gd = NULL;
    if (ProSurfaceDataGet(s, &gd) != PRO_TK_NO_ERROR || !gd ||
        gd->obj_type != PRO_SURFACE || (PRO_SURF_DATA(gd)) == NULL)
        return PRO_TK_NO_ERROR;
    ProSurfacedata *sd = PRO_SURF_DATA(gd);
    if (sd->type == PRO_SRF_PLANE) {
        ProPlanedata *pl = &sd->srf_shape.plane;
        (void)pl;
        /* xyz_min/max 为模型空间近似包围盒（ProSurfacedata.h 文档）。
           v1 仅取法向 ±Z 的放置面：z 键值取 z 向极值，中心取 xy 中点。 */
        double zmin = sd->xyz_min[2];
        double zmax = sd->xyz_max[2];
        double key;
        int better;
        if (strcmp(ctx->face, "top") == 0) {
            key = zmax;
            better = (!ctx->found || key > ctx->best_z);
        } else {
            key = zmin;
            better = (!ctx->found || key < ctx->best_z);
        }
        if (better) {
            ctx->found = 1;
            ctx->best_z = key;
            ProSurfaceIdGet(s, &ctx->best_id);
            ctx->center[0] = (sd->xyz_min[0] + sd->xyz_max[0]) / 2.0;
            ctx->center[1] = (sd->xyz_min[1] + sd->xyz_max[1]) / 2.0;
        }
    }
    ProGeomitemdataFree(&gd);
    return PRO_TK_NO_ERROR;
}

/*---- datum 平面按名查找（RIGHT/FRONT），并取几何中心偏移 ----*/
static int find_datum_plane(ProSolid solid, const char *name, ProModelitem *out)
{
    ProName wname;
    ProStringToWstring(wname, (char *)name);
    ProError e = ProModelitemByNameInit((ProMdl)solid, PRO_FEATURE, wname, out);
    if (e != PRO_TK_NO_ERROR) {
        aicad_error("FS204", "找不到 datum 平面 %s（rc=%d；需默认模板含标准基准面）", name, (int)e);
        return -1;
    }
    ProFeattype ftype;
    e = ProFeatureTypeGet((ProFeature *)out, &ftype);
    if (e != PRO_TK_NO_ERROR || ftype != (ProFeattype)PRO_FEAT_DATUM) {
        aicad_error("FS204", "名为 %s 的特征不是 datum（type=%d）", name, (int)ftype);
        return -1;
    }
    return 0;
}

/*---- 默认基准：CSYS + TOP/FRONT/RIGHT -------------------------------
   ProSolidMdlnameCreate 产出的是空零件（无 CSYS、无基准面），而首特征
   sketch@TOP 与孔的线性放置参考都要求这些基准存在。做法照官方示例
   TestDtmPln.c：先建 PRO_CSYS_DEFAULT 坐标系，再用 PRO_DTMPLN_DEF_X/Y/Z
   建三个正交基准面（无需参考几何，故 headless 可用）。
   ponytail: 三平面固定建在源点、轴向按 DEF_* 默认方向；不做偏移/角度，
   需要非零偏移时升级为 PRO_DTMPLN_OFFS + PRO_E_DTMPLN_CONSTR_REF_OFFSET。
-------------------------------------------------------------------*/
static int create_default_datums(ProSolid solid)
{
    static const struct { int constr; const char *name; } PLANES[3] = {
        { PRO_DTMPLN_DEF_X, "RIGHT" },
        { PRO_DTMPLN_DEF_Y, "FRONT" },
        { PRO_DTMPLN_DEF_Z, "TOP"   },
    };
    ProError e, status;
    ProErrorlist errors;
    ProModelitem model_item;
    ProSelection model_sel;
    ProFeatureCreateOptions *opts = NULL;
    ProFeature feature;

    /* 1) 默认坐标系（官方示例中它是基准面的前置条件） */
    {
        ProElement tree, e_type, e_method;
        CHK(ProElementAlloc(PRO_E_FEATURE_TREE, &tree));
        CHK(ProElementAlloc(PRO_E_FEATURE_TYPE, &e_type));
        CHK(ProElementIntegerSet(e_type, PRO_FEAT_CSYS));
        CHK(ProElemtreeElementAdd(tree, NULL, e_type));
        CHK(ProElementAlloc(PRO_E_CSYS_METHOD, &e_method));
        CHK(ProElementIntegerSet(e_method, PRO_CSYS_DEFAULT));
        CHK(ProElemtreeElementAdd(tree, NULL, e_method));
        CHK(ProMdlToModelitem((ProMdl)solid, &model_item));
        CHK(ProSelectionAlloc(NULL, &model_item, &model_sel));
        e = ProArrayAlloc(1, sizeof(ProFeatureCreateOptions), 1, (ProArray *)&opts);
        if (e != PRO_TK_NO_ERROR) return (int)e;
        opts[0] = PRO_FEAT_CR_NO_OPTS;
        status = ProFeatureWithoptionsCreate(model_sel, tree, opts, PRO_REGEN_NO_FLAGS,
                                             &feature, &errors);
        ProArrayFree((ProArray *)&opts);
        ProElementFree(&tree);
        if (status != PRO_TK_NO_ERROR) {
            aicad_error("FS206", "ProFeatureWithoptionsCreate（默认 CSYS）rc=%d", (int)status);
            return (int)status;
        }
    }

    /* 2) 三个基准面，按名命名（TOP/FRONT/RIGHT） */
    for (int i = 0; i < 3; i++) {
        ProElement tree, e_type, e_consts, e_const, e_ctype;
        ProName wname;
        CHK(ProElementAlloc(PRO_E_FEATURE_TREE, &tree));
        CHK(ProElementAlloc(PRO_E_FEATURE_TYPE, &e_type));
        CHK(ProElementIntegerSet(e_type, PRO_FEAT_DATUM));
        CHK(ProElemtreeElementAdd(tree, NULL, e_type));
        CHK(ProElementAlloc(PRO_E_DTMPLN_CONSTRAINTS, &e_consts));
        CHK(ProElemtreeElementAdd(tree, NULL, e_consts));
        CHK(ProElementAlloc(PRO_E_DTMPLN_CONSTRAINT, &e_const));
        CHK(ProElemtreeElementAdd(e_consts, NULL, e_const));
        CHK(ProElementAlloc(PRO_E_DTMPLN_CONSTR_TYPE, &e_ctype));
        CHK(ProElementIntegerSet(e_ctype, PLANES[i].constr));
        CHK(ProElemtreeElementAdd(e_const, NULL, e_ctype));
        CHK(ProMdlToModelitem((ProMdl)solid, &model_item));
        CHK(ProSelectionAlloc(NULL, &model_item, &model_sel));
        e = ProArrayAlloc(1, sizeof(ProFeatureCreateOptions), 1, (ProArray *)&opts);
        if (e != PRO_TK_NO_ERROR) return (int)e;
        opts[0] = PRO_FEAT_CR_NO_OPTS;
        status = ProFeatureWithoptionsCreate(model_sel, tree, opts, PRO_REGEN_NO_FLAGS,
                                             &feature, &errors);
        ProArrayFree((ProArray *)&opts);
        ProElementFree(&tree);
        if (status != PRO_TK_NO_ERROR) {
            aicad_error("FS206", "ProFeatureWithoptionsCreate（基准面 %s）rc=%d",
                        PLANES[i].name, (int)status);
            return (int)status;
        }
        ProStringToWstring(wname, (char *)PLANES[i].name);
        ProModelitemNameSet((ProModelitem *)&feature, wname);
    }
    aicad_info("默认基准已创建：CSYS + RIGHT/FRONT/TOP");
    return 0;
}

/*---- 直孔创建（元素树 = UgHoleCreate.c；放置参考程序化解析） ----*/
static int create_one_hole(ProSolid solid, const CHole *h, double cx, double cy)
{
    ProError e, status;
    ProErrorlist errors;
    ProModelitem model_item, datum_right, datum_front, face_item;
    ProSelection model_sel, face_sel, ref_sel1, ref_sel2;
    ProReference ref1, ref2;
    ProFeatureCreateOptions *opts = NULL;
    ProElement tree, e_type, e_form, e_com, e_type_new, e_dia;
    ProElement e_depth, e_to, e_to_type, e_from, e_from_type;
    ProElement e_place, e_prim, e_pl_type, e_r1, e_d1, e_r2, e_d2;
    ProFeature feature;

    CHK(ProElementAlloc(PRO_E_FEATURE_TREE, &tree));
    CHK(ProElementAlloc(PRO_E_FEATURE_TYPE, &e_type));
    CHK(ProElementIntegerSet(e_type, PRO_FEAT_HOLE));
    CHK(ProElemtreeElementAdd(tree, NULL, e_type));
    CHK(ProElementAlloc(PRO_E_FEATURE_FORM, &e_form));
    CHK(ProElementIntegerSet(e_form, PRO_HLE_TYPE_STRAIGHT));
    CHK(ProElemtreeElementAdd(tree, NULL, e_form));

    CHK(ProElementAlloc(PRO_E_HLE_COM, &e_com));
    CHK(ProElemtreeElementAdd(tree, NULL, e_com));
    CHK(ProElementAlloc(PRO_E_HLE_TYPE_NEW, &e_type_new));
    CHK(ProElementIntegerSet(e_type_new, PRO_HLE_NEW_TYPE_STRAIGHT));
    CHK(ProElemtreeElementAdd(e_com, NULL, e_type_new));
    CHK(ProElementAlloc(PRO_E_DIAMETER, &e_dia));
    CHK(ProElementDoubleSet(e_dia, h->dia));
    CHK(ProElemtreeElementAdd(e_com, NULL, e_dia));

    /* 深度：通孔（IR v1 无孔深字段） */
    CHK(ProElementAlloc(PRO_E_HOLE_STD_DEPTH, &e_depth));
    CHK(ProElemtreeElementAdd(e_com, NULL, e_depth));
    CHK(ProElementAlloc(PRO_E_HOLE_DEPTH_TO, &e_to));
    CHK(ProElemtreeElementAdd(e_depth, NULL, e_to));
    CHK(ProElementAlloc(PRO_E_HOLE_DEPTH_TO_TYPE, &e_to_type));
    CHK(ProElementIntegerSet(e_to_type, PRO_HLE_STRGHT_THRU_ALL_DEPTH));
    CHK(ProElemtreeElementAdd(e_to, NULL, e_to_type));
    CHK(ProElementAlloc(PRO_E_HOLE_DEPTH_FROM, &e_from));
    CHK(ProElemtreeElementAdd(e_depth, NULL, e_from));
    CHK(ProElementAlloc(PRO_E_HOLE_DEPTH_FROM_TYPE, &e_from_type));
    CHK(ProElementIntegerSet(e_from_type, PRO_HLE_STRGHT_NONE_DEPTH));
    CHK(ProElemtreeElementAdd(e_from, NULL, e_from_type));

    /* 放置：主参考 = 放置面；线性参考 = RIGHT/FRONT datum，孔心偏移 */
    CHK(ProElementAlloc(PRO_E_HLE_PLACEMENT, &e_place));
    CHK(ProElemtreeElementAdd(tree, NULL, e_place));

    if (find_datum_plane(solid, "RIGHT", &datum_right) != 0) return -1;
    if (find_datum_plane(solid, "FRONT", &datum_front) != 0) return -1;

    face_item.owner = (ProMdl)solid;
    face_item.type = PRO_SURFACE;
    /* 每次创建都重新解析放置面（顺带校验面仍存在） */
    FaceCtx fctx;
    memset(&fctx, 0, sizeof(fctx));
    fctx.solid = solid;
    fctx.face = h->on_face[0] ? h->on_face : "top";
    e = ProSolidSurfaceVisit(solid, face_action, face_filter, (ProAppData)&fctx);
    if (e != PRO_TK_NO_ERROR || !fctx.found) {
        aicad_error("FS205", "未找到可用放置面（face=%s, visit rc=%d）", fctx.face, (int)e);
        return -1;
    }
    face_item.id = fctx.best_id;

    CHK(ProSelectionAlloc(NULL, &face_item, &face_sel));

    CHK(ProElementAlloc(PRO_E_HLE_PRIM_REF, &e_prim));
    CHK(ProSelectionToReference(face_sel, &ref1));
    CHK(ProElementReferenceSet(e_prim, ref1));
    CHK(ProElemtreeElementAdd(e_place, NULL, e_prim));

    CHK(ProElementAlloc(PRO_E_HLE_PL_TYPE, &e_pl_type));
    CHK(ProElementIntegerSet(e_pl_type, PRO_HLE_PL_TYPE_LIN));
    CHK(ProElemtreeElementAdd(e_place, NULL, e_pl_type));

    CHK(ProSelectionAlloc(NULL, &datum_right, &ref_sel1));
    CHK(ProSelectionAlloc(NULL, &datum_front, &ref_sel2));

    CHK(ProElementAlloc(PRO_E_HLE_DIM_REF1, &e_r1));
    CHK(ProSelectionToReference(ref_sel1, &ref1));
    CHK(ProElementReferenceSet(e_r1, ref1));
    CHK(ProElemtreeElementAdd(e_place, NULL, e_r1));
    CHK(ProElementAlloc(PRO_E_HLE_DIM_DIST1, &e_d1));
    CHK(ProElementDoubleSet(e_d1, cx));
    CHK(ProElemtreeElementAdd(e_place, NULL, e_d1));

    CHK(ProElementAlloc(PRO_E_HLE_DIM_REF2, &e_r2));
    CHK(ProSelectionToReference(ref_sel2, &ref2));
    CHK(ProElementReferenceSet(e_r2, ref2));
    CHK(ProElemtreeElementAdd(e_place, NULL, e_r2));
    CHK(ProElementAlloc(PRO_E_HLE_DIM_DIST2, &e_d2));
    CHK(ProElementDoubleSet(e_d2, cy));
    CHK(ProElemtreeElementAdd(e_place, NULL, e_d2));

    /* 创建（孔不是首特征，用 DEFINE_MISS_ELEMS 直接完成） */
    CHK(ProMdlToModelitem((ProMdl)solid, &model_item));
    CHK(ProSelectionAlloc(NULL, &model_item, &model_sel));
    e = ProArrayAlloc(1, sizeof(ProFeatureCreateOptions), 1, (ProArray *)&opts);
    if (e != PRO_TK_NO_ERROR) return (int)e;
    opts[0] = PRO_FEAT_CR_DEFINE_MISS_ELEMS;
    status = ProFeatureWithoptionsCreate(model_sel, tree, opts, PRO_REGEN_NO_FLAGS,
                                         &feature, &errors);
    ProArrayFree((ProArray *)&opts);
    ProElementFree(&tree);
    if (status != PRO_TK_NO_ERROR) {
        aicad_error("FS206", "ProFeatureWithoptionsCreate（hole %s）rc=%d", h->id, (int)status);
        dump_pro_errors(&errors);
        return (int)status;
    }
    aicad_map("%s -> ProFeature id=%d（dia=%g @ %.2f,%.2f）", h->id, feature.id, h->dia, cx, cy);
    return 0;
}

/*---- 特征树回读校验 ----*/
static ProError count_feature(ProFeature *f, ProError st, ProAppData data)
{
    (void)f;
    if (st == PRO_TK_NO_ERROR) (*(int *)data)++;
    return PRO_TK_NO_ERROR;
}

typedef struct {
    int total, datums, csys;
} DumpCtx;

static ProError dump_feature(ProFeature *f, ProError st, ProAppData data)
{
    DumpCtx *ctx = (DumpCtx *)data;
    ProFeattype ftype = (ProFeattype)0;
    if (st != PRO_TK_NO_ERROR) return PRO_TK_NO_ERROR;
    ctx->total++;
    /* ProFeature.type 是内部位掩码（打印恒为 3），真实类型须用 ProFeatureTypeGet */
    if (ProFeatureTypeGet(f, &ftype) != PRO_TK_NO_ERROR) ftype = (ProFeattype)f->type;
    if (ftype == (ProFeattype)PRO_FEAT_DATUM) ctx->datums++;
    if (ftype == (ProFeattype)PRO_FEAT_CSYS) ctx->csys++;
    aicad_info("tree: id=%-4d type=%d", f->id, (int)ftype);
    return PRO_TK_NO_ERROR;
}

/*---- 会话零件清空 ------------------------------------------------------
   常驻 rpc_input 会话跨次运行会残留同名模型（Creo 还会自动加 0001 后缀），
   ProSolidMdlnameCreate 因此返回 PRO_TK_E_FOUND(-5)。无界面构建会话每次
   构建都从干净会话开始，故先擦除会话内全部零件。
-------------------------------------------------------------------*/
static int erase_session_parts(void)
{
    ProMdl *list = NULL;
    int n = 0, erased = 0;
    if (ProSessionMdlList(PRO_MDL_PART, &list, &n) != PRO_TK_NO_ERROR)
        return 0;
    for (int i = 0; i < n; i++) {
        ProName nm;
        if (ProMdlNameGet(list[i], nm) == PRO_TK_NO_ERROR)
            aicad_info("  擦除会话残留零件：%ls", nm);
        ProMdlErase(list[i]);
        erased++;
    }
    ProArrayFree((ProArray *)&list);
    return erased;
}

/* ==================================================================== *\
   main
\* ==================================================================== */

/* OTK↔C API 混合模式连接桥（实现见 otk_bridge.cpp） */
extern int aicad_bridge_connect(const char *text_dir);
extern int aicad_bridge_disconnect(int dwell_sec);
static int g_bridge = 0; /* 1 = 连接由 OTK 桥建立（此时无 C API process handle） */

int main(int argc, char *argv[])
{
    ProError e;
    ProProcessHandle handle;
    ProBoolean random = PRO_B_FALSE;
    ProMdlName part_name;
    ProSolid solid = NULL;
    ProFeature feat;
    const char *spec_path = NULL;
    const char *text_dir = NULL;
    char *json_text = NULL;
    CSpec spec;
    JParser jp;
    int rc = 0;

    memset(&handle, 0, sizeof(handle));
    /* 无缓冲 stdout：headless 会话里 Creo 会 EXCEPTION_ACCESS_VIOLATION，
       缓冲日志会随之丢失，排障必须看到崩溃前的最后几行。 */
    setvbuf(stdout, NULL, _IONBF, 0);
    if (argc >= 2) spec_path = argv[1];
    if (argc >= 3) text_dir = argv[2];
    if (!spec_path) {
        fprintf(stderr, "用法: feat_build <spec.json> [text_dir]\n");
        return 2;
    }

    /* 1. 读取 spec -------------------------------------------------------- */
    {
        FILE *fp = fopen(spec_path, "rb");
        if (!fp) { aicad_error("FS200", "无法读取 spec 文件：%s", spec_path); return 2; }
        fseek(fp, 0, SEEK_END);
        long sz = ftell(fp);
        fseek(fp, 0, SEEK_SET);
        if (sz <= 0 || sz > 1024 * 1024) {
            aicad_error("FS200", "spec 文件大小异常（%ld bytes）", sz);
            fclose(fp);
            return 2;
        }
        json_text = (char *)malloc((size_t)sz + 1);
        if (!json_text) { fclose(fp); aicad_error("FS200", "内存不足"); return 2; }
        if (fread(json_text, 1, (size_t)sz, fp) != (size_t)sz) {
            aicad_error("FS200", "spec 文件读取失败");
            free(json_text);
            fclose(fp);
            return 2;
        }
        json_text[sz] = 0;
        fclose(fp);
    }

    /* 2. 解析 + 校验 ------------------------------------------------------- */
    {
        int root = j_parse(json_text, &jp);
        free(json_text);
        if (root < 0) { free(jp.nodes); return 2; }
        int spec_ok = (spec_from_json(&jp, root, &spec) == 0);
        free(jp.nodes);
        if (!spec_ok) return 2;
    }
    aicad_info("spec 校验通过：model=%s, sketch=%s(%gx%g@%s), extrude=%s(depth=%g,%s), holes=%d, edits=%d",
               spec.model, spec.sketch.id, spec.sketch.w, spec.sketch.h, spec.sketch.plane,
               spec.extrude.id, spec.extrude.depth,
               spec.extrude.symmetric ? "symmetric" : "one_side",
               spec.n_holes, spec.n_edits);

    /* 3. 连接无界面 Creo ----------------------------------------------------
       实测（本机 Creo 12.4.0.0）：C API ProEngineerConnect 对
       `-g:no_graphics -i:rpc_input` 会话恒返回 -4（PRO_TK_E_NOT_FOUND），
       与 native/r1_otk/README.md 记录一致；且传 NULL 作 session_id 的形态会
       让进程异常退出。故走文档既定的混合模式（ROADMAP P1）：
       OTK pfcAsyncConnection::Connect 建会话通道（R1 已验证可连 rpc_input），
       本进程继续用 C Toolkit 建特征（otk_bridge.cpp）。 */
    if (aicad_bridge_connect(text_dir) == 0) {
        g_bridge = 1;
        aicad_info("已连接无界面 Creo（OTK 混合模式）");
    } else {
        aicad_error("FS209", "OTK 桥连接失败（C API 对 rpc_input 恒为 -4，见 r1_otk/README.md）");
        return 3;
    }

    /* 4. 建零件 ------------------------------------------------------------ */
    ProStringToWstring(part_name, spec.model);
    e = ProSolidMdlnameCreate(part_name, PRO_MDLFILE_PART, &solid);
    if (e == PRO_TK_E_FOUND) {
        erase_session_parts(); /* 常驻会话残留同名模型：清空后重建 */
        e = ProSolidMdlnameCreate(part_name, PRO_MDLFILE_PART, &solid);
    }
    if (e != PRO_TK_NO_ERROR) {
        aicad_error("FS206", "ProSolidMdlnameCreate(%s) rc=%d", spec.model, (int)e);
        if (g_bridge) aicad_bridge_disconnect(30);
    else ProEngineerDisconnect(&handle, 30);
        return 5;
    }
    aicad_info("零件已创建：%s.prt", spec.model);

    /* 4b. 默认基准（空零件无 CSYS/基准面，首特征与孔放置都依赖它们） -------- */
    if (create_default_datums(solid) != 0) {
        if (g_bridge) aicad_bridge_disconnect(30);
        else ProEngineerDisconnect(&handle, 30);
        return 5;
    }
    {
        DumpCtx dc;
        memset(&dc, 0, sizeof(dc));
        ProSolidFeatVisit(solid, dump_feature, NULL, (ProAppData)&dc);
        aicad_info("基准创建后特征数=%d（datum=%d）", dc.total, dc.datums);
    }

    /* 5. 首特征 sketch+extrude --------------------------------------------- */
    if (create_first_extrude(solid, &spec, &feat) != 0) {
        if (g_bridge) aicad_bridge_disconnect(30);
    else ProEngineerDisconnect(&handle, 30);
        return 5;
    }

    /* 6. 孔（含 pattern 多实例） -------------------------------------------- */
    for (int hi = 0; hi < spec.n_holes; hi++) {
        CHole *h = &spec.holes[hi];
        FaceCtx fctx;
        memset(&fctx, 0, sizeof(fctx));
        fctx.face = h->on_face[0] ? h->on_face : "top";
        e = ProSolidSurfaceVisit(solid, face_action, face_filter, (ProAppData)&fctx);
        if (e != PRO_TK_NO_ERROR || !fctx.found) {
            aicad_error("FS205", "hole %s：未找到放置面（face=%s, visit rc=%d）", h->id, fctx.face, (int)e);
            rc = 5;
            break;
        }
        int n1 = h->has_pattern ? h->pat_n[0] : 1;
        int n2 = h->has_pattern ? h->pat_n[1] : 1;
        double p1 = h->has_pattern ? h->pat_pitch[0] : 0.0;
        double p2 = h->has_pattern ? h->pat_pitch[1] : 0.0;
        int failed = 0;
        for (int i = 0; i < n1 && !failed; i++) {
            for (int j = 0; j < n2 && !failed; j++) {
                double ox = ((double)i - (n1 - 1) / 2.0) * p1;
                double oy = ((double)j - (n2 - 1) / 2.0) * p2;
                if (create_one_hole(solid, h, fctx.center[0] + ox, fctx.center[1] + oy) != 0)
                    failed = 1;
            }
        }
        if (failed) { rc = 5; break; }
    }

    /* 7. 再生 + 特征树校验 --------------------------------------------------- */
    if (rc == 0) {
        e = ProSolidRegenerate(solid, PRO_REGEN_NO_FLAGS);
        if (e != PRO_TK_NO_ERROR) {
            aicad_error("FS207", "ProSolidRegenerate rc=%d", (int)e);
            rc = 7;
        }
    }
    /* 几何自检（regen 之后体积才可取）：体积 = 草绘面积 x 深度。
       若对称深度被当成"半深"，体积会翻倍 —— 这是拉伸语义唯一的静默失效点。 */
    if (rc == 0) {
        ProMassProperty mass;
        double expect = spec.sketch.w * spec.sketch.h * spec.extrude.depth;
        memset(&mass, 0, sizeof(mass));
        e = ProSolidMassPropertyGet(solid, NULL, &mass);
        if (e != PRO_TK_NO_ERROR) {
            aicad_error("FS207", "ProSolidMassPropertyGet rc=%d", (int)e);
            rc = 7;
        } else {
            aicad_info("体积 %.3f（期望 %.3f = %.0f x %.0f x %.3f）",
                       mass.volume, expect, spec.sketch.w, spec.sketch.h, spec.extrude.depth);
            if (expect > 0.0 && fabs(mass.volume - expect) / expect > 0.01) {
                aicad_error("FS207", "拉伸深度几何不符：体积 %.3f，期望 %.3f",
                            mass.volume, expect);
                rc = 8;
            }
        }
    }
    if (rc == 0) {
        DumpCtx ctx;
        memset(&ctx, 0, sizeof(ctx));
        ProSolidFeatVisit(solid, dump_feature, NULL, (ProAppData)&ctx);
        int solid_feats = ctx.total - ctx.datums - ctx.csys; /* 不计基准面/坐标系 */
        int expected = 1;                          /* 首特征对算 1 个 ProFeature */
        for (int hi = 0; hi < spec.n_holes; hi++)
            expected += (spec.holes[hi].has_pattern ? spec.holes[hi].pat_n[0] * spec.holes[hi].pat_n[1] : 1);
        aicad_info("特征树校验：实体特征 %d（另有 datum %d, csys %d），预期 %d",
                   solid_feats, ctx.datums, ctx.csys, expected);
        if (solid_feats != expected) {
            aicad_error("FS214", "特征树与 FeatureSpec 不一致：实际 %d，预期 %d（G1/G3 验收点）",
                        solid_feats, expected);
            rc = 8;
        }
    }

    /* 8. 保存 --------------------------------------------------------------- */
    if (rc == 0) {
        e = ProMdlSave((ProMdl)solid);
        if (e != PRO_TK_NO_ERROR) {
            aicad_error("FS208", "ProMdlSave rc=%d", (int)e);
            rc = 9;
        } else {
            aicad_info("已保存 %s.prt（原生特征树，G1/G4 验收产物）", spec.model);
        }
    }

    if (g_bridge) aicad_bridge_disconnect(30);
    else ProEngineerDisconnect(&handle, 30);
    aicad_info("已断开（Creo 保持运行）");
    return rc;
}
