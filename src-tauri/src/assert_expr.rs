//! 校验表达式解析（FR-C5）
//!
//! 语法：`<提取器> <比较符> <期望值>`，多条规则用 AND 连接（第一版仅支持 AND）。
//! 示例：`status == 200 AND jsonpath:$.code == 0 AND jsonpath:$.msg contains 成功`
//!
//! 本模块只做**解析与校验**，不负责求值；M5 执行引擎会复用 Rule 结构做实际比对。

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// 提取器：从响应中提取待比对的值
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum Extractor {
    /// HTTP 状态码
    Status,
    /// JSON 路径，如 `$.data.id`
    JsonPath(String),
    /// 响应头，如 `X-Token`
    Header(String),
}

/// 比较符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
}

impl CompareOp {
    pub fn symbol(&self) -> &'static str {
        match self {
            CompareOp::Eq => "==",
            CompareOp::Ne => "!=",
            CompareOp::Gt => ">",
            CompareOp::Ge => ">=",
            CompareOp::Lt => "<",
            CompareOp::Le => "<=",
            CompareOp::Contains => "contains",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "==" => Some(CompareOp::Eq),
            "!=" => Some(CompareOp::Ne),
            ">" => Some(CompareOp::Gt),
            ">=" => Some(CompareOp::Ge),
            "<" => Some(CompareOp::Lt),
            "<=" => Some(CompareOp::Le),
            // contains 允许大小写混用
            s if s.eq_ignore_ascii_case("contains") => Some(CompareOp::Contains),
            _ => None,
        }
    }

    /// 数值比较符：期望值必须是数字或动态函数
    pub fn is_numeric(self) -> bool {
        matches!(self, CompareOp::Gt | CompareOp::Ge | CompareOp::Lt | CompareOp::Le)
    }
}

/// 单条校验规则
#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub extractor: Extractor,
    pub op: CompareOp,
    pub expected: String,
}

impl Rule {
    /// 还原为规范化的表达式文本（M5 报告展示用；当前仅测试覆盖）
    #[allow(dead_code)]
    pub fn to_expr(&self) -> String {
        let ext = match &self.extractor {
            Extractor::Status => "status".to_string(),
            Extractor::JsonPath(p) => format!("jsonpath:{p}"),
            Extractor::Header(h) => format!("header:{h}"),
        };
        format!("{} {} {}", ext, self.op.symbol(), self.expected)
    }
}

/// 单条校验结果（执行引擎落库 / 前端展示用）
#[derive(Debug, Clone, Serialize)]
pub struct AssertItem {
    /// 规范化后的规则表达式
    pub rule: String,
    /// 判定：pass（通过）/ fail（失败）/ skipped（不适用）
    pub verdict: String,
    /// 实际提取值（字符串形态）
    pub extracted: String,
    /// 期望值
    pub expected: String,
    /// 说明（失败原因 / 跳过原因）
    pub message: String,
}

/// 简化 JSONPath 提取（第一版仅支持 `$.a.b[0].c`，无过滤器/递归）
pub fn jsonpath_simple(path: &str, root: &Value) -> Result<Value, String> {
    let p = path.trim();
    let p = p.strip_prefix("$.").or_else(|| p.strip_prefix("$")).unwrap_or(p);
    if p.is_empty() {
        return Ok(root.clone());
    }
    let chars: Vec<char> = p.chars().collect();
    let mut i = 0;
    let mut cur = root;
    while i < chars.len() {
        let c = chars[i];
        if c == '.' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            if name.is_empty() {
                return Err(format!("JSONPath 语法错误：空字段名（{path}）"));
            }
            match cur.get(&name) {
                Some(v) => cur = v,
                None => return Err(format!("JSONPath 字段「{name}」不存在（{path}）")),
            }
        } else if c == '[' {
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != ']' {
                i += 1;
            }
            if i >= chars.len() {
                return Err(format!("JSONPath 缺少 ]（{path}）"));
            }
            let num: String = chars[start..i].iter().collect();
            i += 1; // 跳过 ]
            let idx: usize = num
                .trim()
                .parse()
                .map_err(|_| format!("JSONPath 索引「{num}」不是数字（{path}）"))?;
            match cur.get(idx) {
                Some(v) => cur = v,
                None => return Err(format!("JSONPath 索引 [{idx}] 越界（{path}）")),
            }
        } else {
            // 首字段（无前导点）
            let start = i;
            while i < chars.len() && chars[i] != '.' && chars[i] != '[' {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            if name.is_empty() {
                return Err(format!("JSONPath 语法错误（{path}）"));
            }
            match cur.get(&name) {
                Some(v) => cur = v,
                None => return Err(format!("JSONPath 字段「{name}」不存在（{path}）")),
            }
        }
    }
    Ok(cur.clone())
}

/// 期望值中的动态函数解析：now() → 当前 Unix 秒（字符串）
fn resolve_expected(expected: &str) -> String {
    if expected.trim() == "now()" {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        secs.to_string()
    } else {
        expected.to_string()
    }
}

/// 比较判定：返回 pass / fail
fn compare(op: CompareOp, actual: &str, expected: &str) -> &'static str {
    let exp = resolve_expected(expected);
    let a_num = actual.parse::<f64>();
    let e_num = exp.parse::<f64>();
    match op {
        CompareOp::Eq => {
            if let (Ok(a), Ok(e)) = (a_num, e_num) {
                if a == e { "pass" } else { "fail" }
            } else if actual == exp.as_str() {
                "pass"
            } else {
                "fail"
            }
        }
        CompareOp::Ne => {
            if let (Ok(a), Ok(e)) = (a_num, e_num) {
                if a != e { "pass" } else { "fail" }
            } else if actual != exp.as_str() {
                "pass"
            } else {
                "fail"
            }
        }
        CompareOp::Contains => {
            if actual.contains(&exp) {
                "pass"
            } else {
                "fail"
            }
        }
        CompareOp::Gt => num_cmp(a_num, e_num, |a, e| a > e),
        CompareOp::Ge => num_cmp(a_num, e_num, |a, e| a >= e),
        CompareOp::Lt => num_cmp(a_num, e_num, |a, e| a < e),
        CompareOp::Le => num_cmp(a_num, e_num, |a, e| a <= e),
    }
}

fn num_cmp(a: Result<f64, std::num::ParseFloatError>, e: Result<f64, std::num::ParseFloatError>, f: impl Fn(f64, f64) -> bool) -> &'static str {
    match (a, e) {
        (Ok(a), Ok(e)) => {
            if f(a, e) {
                "pass"
            } else {
                "fail"
            }
        }
        _ => "fail",
    }
}

/// 对单条规则求值
pub fn eval(
    rule: &Rule,
    status: u16,
    body: Option<&Value>,
    headers: &HashMap<String, String>,
) -> AssertItem {
    let extracted = match &rule.extractor {
        Extractor::Status => status.to_string(),
        Extractor::Header(h) => match headers.get(h) {
            Some(v) => v.clone(),
            None => {
                return AssertItem {
                    rule: rule.to_expr(),
                    verdict: "fail".into(),
                    extracted: "".into(),
                    expected: rule.expected.clone(),
                    message: format!("响应头 {h} 不存在"),
                }
            }
        },
        Extractor::JsonPath(p) => match body {
            None => {
                return AssertItem {
                    rule: rule.to_expr(),
                    verdict: "skipped".into(),
                    extracted: "".into(),
                    expected: rule.expected.clone(),
                    message: "响应体非 JSON，跳过".into(),
                }
            }
            Some(v) => match jsonpath_simple(p, v) {
                Ok(val) => match val {
                    Value::String(s) => s,
                    other => other.to_string(),
                },
                Err(e) => {
                    return AssertItem {
                        rule: rule.to_expr(),
                        verdict: "fail".into(),
                        extracted: "".into(),
                        expected: rule.expected.clone(),
                        message: e,
                    }
                }
            },
        },
    };

    let verdict = compare(rule.op, &extracted, &rule.expected);
    let message = match verdict {
        "pass" => "通过".into(),
        _ => format!(
            "期望 {} {}，实际 {}",
            rule.op.symbol(),
            rule.expected,
            extracted
        ),
    };
    AssertItem {
        rule: rule.to_expr(),
        verdict: verdict.into(),
        extracted,
        expected: rule.expected.clone(),
        message,
    }
}

/// 按 AND 切分（大小写不敏感），同时支持换行分隔
fn split_and(input: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in input.lines() {
        let mut rest = line.trim();
        loop {
            let lower = rest.to_lowercase();
            match lower.find(" and ") {
                Some(i) => {
                    out.push(rest[..i].trim().to_string());
                    rest = rest[i + 5..].trim();
                }
                None => {
                    out.push(rest.to_string());
                    break;
                }
            }
        }
    }
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

/// 判断期望值是否为动态函数（第一版：now() / len(...)）
fn is_dynamic_fn(s: &str) -> bool {
    let s = s.trim();
    (s.starts_with("now(") || s.starts_with("len(")) && s.ends_with(')')
}

/// 解析校验表达式。
/// - 空表达式（未填写校验表达式）返回默认规则集 `[status == 200]`：
///   即未填时默认仅判断 HTTP 状态码等于 200。
/// - 非空表达式按 AND 拆分为多条规则。
pub fn parse(expr: &str) -> Result<Vec<Rule>, String> {
    if expr.trim().is_empty() {
        return Ok(vec![Rule {
            extractor: Extractor::Status,
            op: CompareOp::Eq,
            expected: "200".to_string(),
        }]);
    }
    let mut rules = Vec::new();
    for (idx, seg) in split_and(expr).into_iter().enumerate() {
        let rule = parse_one(&seg).map_err(|e| format!("第 {} 条规则「{}」{}", idx + 1, seg, e))?;
        rules.push(rule);
    }
    Ok(rules)
}

fn parse_one(seg: &str) -> Result<Rule, String> {
    let mut tokens = seg.split_whitespace();
    let ext_raw = tokens.next().ok_or("缺少提取器")?;
    let op_raw = tokens.next().ok_or("缺少比较符")?;
    let expected: Vec<&str> = tokens.collect();
    if expected.is_empty() {
        return Err("缺少期望值".to_string());
    }
    let expected = expected.join(" ").trim().to_string();
    if let Some(rest) = expected.strip_prefix("OR ") {
        return Err(format!("暂不支持 OR 组合（第一版仅 AND），请拆分表达式：{rest}"));
    }

    let extractor = if ext_raw.eq_ignore_ascii_case("status") {
        Extractor::Status
    } else if let Some(p) = ext_raw.strip_prefix("jsonpath:") {
        if p.trim().is_empty() {
            return Err("jsonpath 路径不能为空（示例：jsonpath:$.code）".to_string());
        }
        Extractor::JsonPath(p.to_string())
    } else if let Some(h) = ext_raw.strip_prefix("header:") {
        if h.trim().is_empty() {
            return Err("响应头名称不能为空（示例：header:X-Token）".to_string());
        }
        Extractor::Header(h.to_string())
    } else {
        return Err(format!(
            "未知提取器「{ext_raw}」，支持：status / jsonpath:$.x.y / header:X-Token"
        ));
    };

    let op = CompareOp::parse(op_raw)
        .ok_or_else(|| format!("未知比较符「{op_raw}」，支持：== != > >= < <= contains"))?;

    // 数值比较：期望值需为数字或动态函数
    if op.is_numeric() && expected.parse::<f64>().is_err() && !is_dynamic_fn(&expected) {
        return Err(format!(
            "比较符 {} 需要数值型期望值，当前为「{expected}」（支持数字或 now()）",
            op.symbol()
        ));
    }

    Ok(Rule { extractor, op, expected })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_defaults_to_status_200() {
        let rules = parse("").unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].extractor, Extractor::Status);
        assert_eq!(rules[0].op, CompareOp::Eq);
        assert_eq!(rules[0].expected, "200");
        // 纯空白等价空表达式，同样返回默认规则
        assert_eq!(parse("   \n  ").unwrap(), rules);
    }

    #[test]
    fn parse_single_rule() {
        let rules = parse("status == 200").unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].extractor, Extractor::Status);
        assert_eq!(rules[0].op, CompareOp::Eq);
        assert_eq!(rules[0].expected, "200");
    }

    #[test]
    fn parse_multi_rules_and_newline() {
        let rules = parse(
            "status == 200 AND jsonpath:$.code == 0\njsonpath:$.msg contains 成功\n\nheader:X-Token != ",
        );
        // 最后一条缺期望值 → 应报错
        assert!(rules.is_err());

        let rules = parse("status == 200 AND jsonpath:$.code == 0\nheader:X-Token != abc").unwrap();
        assert_eq!(rules.len(), 3);
        assert_eq!(rules[1].extractor, Extractor::JsonPath("$.code".into()));
        assert_eq!(rules[2].op, CompareOp::Ne);
        assert_eq!(rules[2].expected, "abc");
    }

    #[test]
    fn contains_is_case_insensitive() {
        let rules = parse("jsonpath:$.msg CONTAINS 成功").unwrap();
        assert_eq!(rules[0].op, CompareOp::Contains);
    }

    #[test]
    fn reject_bad_extractor_or_op() {
        assert!(parse("foo == 1").err().unwrap().contains("未知提取器"));
        assert!(parse("status === 1").err().unwrap().contains("未知比较符"));
        assert!(parse("jsonpath: == 1").err().unwrap().contains("jsonpath 路径不能为空"));
        assert!(parse("status ==").err().unwrap().contains("缺少期望值"));
    }

    #[test]
    fn numeric_op_requires_number() {
        assert!(parse("status > 199").is_ok());
        assert!(parse("status > abc").err().unwrap().contains("数值型期望值"));
        // 动态函数放行
        assert!(parse("jsonpath:$.expireAt > now()").is_ok());
        assert!(parse("jsonpath:$.list >= len(1)").is_ok());
    }

    #[test]
    fn to_expr_roundtrip() {
        let expr = "status == 200 AND jsonpath:$.code == 0 AND header:X-Token != x";
        let rules = parse(expr).unwrap();
        let back = rules.iter().map(Rule::to_expr).collect::<Vec<_>>().join(" AND ");
        assert_eq!(back, expr);
    }

    #[test]
    fn jsonpath_extracts_nested() {
        let v: Value = serde_json::from_str(r#"{"code":0,"data":{"id":42,"list":["a","b","c"]},"msg":"成功"}"#).unwrap();
        assert_eq!(jsonpath_simple("$.code", &v).unwrap(), jsonpath_simple("code", &v).unwrap());
        assert_eq!(jsonpath_simple("$.code", &v).unwrap(), Value::from(0));
        assert_eq!(jsonpath_simple("$.data.id", &v).unwrap(), Value::from(42));
        assert_eq!(jsonpath_simple("$.data.list[1]", &v).unwrap(), Value::from("b"));
        assert!(jsonpath_simple("$.data.missing", &v).is_err());
        assert!(jsonpath_simple("$.data.list[9]", &v).is_err());
    }

    #[test]
    fn eval_status_and_jsonpath() {
        let v: Value = serde_json::from_str(r#"{"code":0,"msg":"成功"}"#).unwrap();
        let mut h = HashMap::new();
        h.insert("X-Token".into(), "abc".into());

        let rules = parse("status == 200 AND jsonpath:$.code == 0 AND jsonpath:$.msg contains 成功").unwrap();
        let results: Vec<AssertItem> = rules
            .iter()
            .map(|r| eval(r, 200, Some(&v), &h))
            .collect();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|a| a.verdict == "pass"));
    }

    #[test]
    fn eval_fails_on_wrong_value() {
        let v: Value = serde_json::from_str(r#"{"code":1}"#).unwrap();
        let rules = parse("status == 200 AND jsonpath:$.code == 0").unwrap();
        let results: Vec<AssertItem> = rules.iter().map(|r| eval(r, 500, Some(&v), &HashMap::new())).collect();
        assert_eq!(results[0].verdict, "fail");
        assert!(results[0].message.contains("期望 =="));
        assert_eq!(results[1].verdict, "fail");
    }

    #[test]
    fn eval_skips_jsonpath_on_non_json() {
        let rules = parse("jsonpath:$.code == 0").unwrap();
        let r = eval(&rules[0], 200, None, &HashMap::new());
        assert_eq!(r.verdict, "skipped");
        assert_eq!(r.message, "响应体非 JSON，跳过");
    }

    #[test]
    fn eval_numeric_comparison() {
        let v: Value = serde_json::from_str(r#"{"count":10}"#).unwrap();
        let rules = parse("jsonpath:$.count > 5 AND jsonpath:$.count <= 10").unwrap();
        let results: Vec<AssertItem> = rules.iter().map(|r| eval(r, 200, Some(&v), &HashMap::new())).collect();
        assert_eq!(results[0].verdict, "pass");
        assert_eq!(results[1].verdict, "pass");

        let rules = parse("jsonpath:$.count > 100").unwrap();
        assert_eq!(eval(&rules[0], 200, Some(&v), &HashMap::new()).verdict, "fail");
    }

    #[test]
    fn eval_header_extraction() {
        let mut h = HashMap::new();
        h.insert("Content-Type".into(), "application/json".into());
        let rules = parse("header:Content-Type contains json").unwrap();
        assert_eq!(eval(&rules[0], 200, None, &h).verdict, "pass");

        let rules = parse("header:Missing == x").unwrap();
        assert_eq!(eval(&rules[0], 200, None, &h).verdict, "fail");
    }
}
