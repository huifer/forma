//! BPMN 2.0 子集适配器。
//! MVP 支持元素：process、startEvent、endEvent、task、userTask、serviceTask、
//! scriptTask、manualTask、exclusiveGateway、sequenceFlow。
//! 自研轻量 XML 解析（元素、属性、文本内容），不引入完整 XML 依赖。

use crate::adapters::{AdapterContext, AdapterResult, SpecAdapter};
use crate::graph::{EdgeKind, NodeKind, SemanticEdge, SemanticNode, Standard, SourceRef};
use crate::identity::{workflow_step_uri, workflow_uri};
use anyhow::Result;

pub struct BpmnAdapter;

const STEP_ELEMENTS: [&str; 13] = [
    "startEvent",
    "endEvent",
    "task",
    "userTask",
    "serviceTask",
    "scriptTask",
    "manualTask",
    "businessRuleTask",
    "sendTask",
    "receiveTask",
    "exclusiveGateway",
    "parallelGateway",
    "inclusiveGateway",
];

impl SpecAdapter for BpmnAdapter {
    fn standard(&self) -> Standard {
        Standard::Bpmn
    }

    fn parse(&self, ctx: &AdapterContext<'_>) -> Result<AdapterResult> {
        let text = crate::util::read_text(&ctx.absolute_path)?;
        let elements = parse_xml_elements(&text);
        let mut result = AdapterResult::default();
        let project = ctx.loaded.manifest.project_name.clone();

        let Some(process) = find_element(&elements, "process") else {
            return Ok(result);
        };
        let process_id = process
            .attrs
            .get("id")
            .cloned()
            .unwrap_or_else(|| "process".to_string());
        let workflow = workflow_uri(&project, &process_id);

        let mut steps: Vec<serde_json::Value> = Vec::new();
        let mut flows: Vec<serde_json::Value> = Vec::new();

        for child in &process.children {
            if STEP_ELEMENTS.contains(&child.name.as_str()) {
                let id = child.attrs.get("id").cloned().unwrap_or_else(|| child.name.clone());
                steps.push(serde_json::json!({
                    "id": id,
                    "name": child.attrs.get("name"),
                    "kind": child.name,
                    "incoming": child.child_texts.get("incoming").cloned().unwrap_or_default(),
                    "outgoing": child.child_texts.get("outgoing").cloned().unwrap_or_default(),
                }));
            }
            if child.name == "sequenceFlow" {
                flows.push(serde_json::json!({
                    "id": child.attrs.get("id").cloned().unwrap_or_else(|| "flow".into()),
                    "sourceRef": child.attrs.get("sourceRef").cloned().unwrap_or_default(),
                    "targetRef": child.attrs.get("targetRef").cloned().unwrap_or_default(),
                    "name": child.attrs.get("name"),
                    "condition": child.condition,
                }));
            }
        }

        result.nodes.push(SemanticNode {
            uri: workflow.clone(),
            kind: NodeKind::Workflow,
            name: process
                .attrs
                .get("name")
                .cloned()
                .unwrap_or_else(|| process_id.clone()),
            standard: Standard::Bpmn,
            source: SourceRef { file: ctx.file.clone(), line: Some(process.line) },
            data: serde_json::json!({
                "processId": process_id,
                "processName": process.attrs.get("name"),
                "steps": steps,
                "flows": flows,
            }),
        });

        for step in &steps {
            let sid = step.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            let step_uri = workflow_step_uri(&project, &process_id, sid);
            result.nodes.push(SemanticNode {
                uri: step_uri.clone(),
                kind: NodeKind::WorkflowStep,
                name: step
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or(sid)
                    .to_string(),
                standard: Standard::Bpmn,
                source: SourceRef { file: ctx.file.clone(), line: None },
                data: step.clone(),
            });
            result.edges.push(SemanticEdge {
                from: workflow.clone(),
                to: step_uri,
                kind: EdgeKind::BelongsTo,
            });
        }

        Ok(result)
    }
}

fn find_element<'a>(els: &'a [XmlElement], name: &str) -> Option<&'a XmlElement> {
    for el in els {
        if el.name == name {
            return Some(el);
        }
        if let Some(found) = find_element(&el.children, name) {
            return Some(found);
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct XmlElement {
    pub name: String,
    pub attrs: std::collections::HashMap<String, String>,
    pub children: Vec<XmlElement>,
    /// 简单文本子元素（如 <incoming>Flow_x</incoming>）
    pub child_texts: std::collections::HashMap<String, Vec<String>>,
    pub text: Option<String>,
    pub condition: Option<String>,
    pub line: usize,
}

/// 极简 XML 解析：元素、属性、文本内容。满足 bpmn.io 导出的常规结构。
/// arena + 索引栈方式规避所有权问题。
pub fn parse_xml_elements(xml: &str) -> Vec<XmlElement> {
    use std::collections::HashMap;

    struct NodeBuilder {
        name: String,
        attrs: HashMap<String, String>,
        children: Vec<usize>,
        text: Option<String>,
        condition: Option<String>,
        line: usize,
    }

    let bytes = xml.as_bytes();
    let n = bytes.len();
    let mut arena: Vec<NodeBuilder> = Vec::new();
    let mut root: Vec<usize> = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (arena idx, content start)
    let mut i = 0usize;

    let line_at = |offset: usize| xml[..offset.min(n)].bytes().filter(|b| *b == b'\n').count() + 1;

    while i < n {
        let Some(lt_rel) = xml[i..].find('<') else { break };
        let lt = i + lt_rel;
        if xml[lt..].starts_with("<!--") {
            i = match xml[lt..].find("-->") {
                Some(end) => lt + end + 3,
                None => n,
            };
            continue;
        }
        if xml[lt..].starts_with("<?") || xml[lt..].starts_with("<!") {
            i = match xml[lt..].find('>') {
                Some(end) => lt + end + 1,
                None => n,
            };
            continue;
        }
        let Some(tag_end_rel) = xml[lt..].find('>') else { break };
        let tag_end = lt + tag_end_rel;
        let tag_content = &xml[lt + 1..tag_end];

        if tag_content.starts_with('/') {
            // 闭合标签：设置元素文本内容
            if let Some((idx, content_start)) = stack.pop() {
                let raw = xml[content_start..lt].trim().to_string();
                if arena[idx].children.is_empty() && !raw.is_empty() {
                    arena[idx].text = Some(decode_xml_entities(&raw));
                    if arena[idx].name == "conditionExpression" {
                        arena[idx].condition = arena[idx].text.clone();
                    }
                }
            }
            i = tag_end + 1;
            continue;
        }

        let self_closing = tag_content.ends_with('/');
        let inner = if self_closing { &tag_content[..tag_content.len() - 1] } else { tag_content };
        let raw_name: String = inner
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '-' | '_'))
            .collect();
        let name = if raw_name.is_empty() { "unknown".to_string() } else { raw_name };

        let mut attrs = HashMap::new();
        parse_attrs(inner, &mut attrs);

        let node = NodeBuilder {
            name: name.rsplit(':').next().unwrap_or(&name).to_string(),
            attrs,
            children: Vec::new(),
            text: None,
            condition: None,
            line: line_at(lt),
        };
        let idx = arena.len();
        arena.push(node);

        if let Some((parent, _)) = stack.last() {
            arena[*parent].children.push(idx);
        } else {
            root.push(idx);
        }

        if !self_closing {
            stack.push((idx, tag_end + 1));
        }
        i = tag_end + 1;
    }

    // 从 arena 组装最终树（含 child_texts）
    fn build(
        arena: &[NodeBuilder],
        idx: usize,
    ) -> XmlElement {
        let node = &arena[idx];
        let mut child_texts: HashMap<String, Vec<String>> = HashMap::new();
        let children: Vec<XmlElement> = node
            .children
            .iter()
            .map(|c| {
                let child = build(arena, *c);
                if child.children.is_empty() {
                    if let Some(t) = &child.text {
                        child_texts.entry(child.name.clone()).or_default().push(t.clone());
                        if child.name == "conditionExpression" {
                            // condition 提升到父元素
                        }
                    }
                }
                child
            })
            .collect();
        let condition = node.condition.clone().or_else(|| {
            children
                .iter()
                .find(|c| c.name == "conditionExpression")
                .and_then(|c| c.text.clone())
        });
        XmlElement {
            name: node.name.clone(),
            attrs: node.attrs.clone(),
            children,
            child_texts,
            text: node.text.clone(),
            condition,
            line: node.line,
        }
    }

    root.into_iter().map(|idx| build(&arena, idx)).collect()
}

fn parse_attrs(inner: &str, attrs: &mut std::collections::HashMap<String, String>) {
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    let n = bytes.len();
    while i < n {
        if bytes[i] == b'=' {
            let name_end = i;
            let mut name_start = name_end;
            while name_start > 0 {
                let c = bytes[name_start - 1] as char;
                if c.is_ascii_alphanumeric() || matches!(c, ':' | '.' | '-' | '_') {
                    name_start -= 1;
                } else {
                    break;
                }
            }
            let mut j = i + 1;
            while j < n && (bytes[j] as char).is_whitespace() {
                j += 1;
            }
            if j < n && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                let val_start = j + 1;
                let mut val_end = val_start;
                while val_end < n && bytes[val_end] != quote {
                    val_end += 1;
                }
                let key = inner[name_start..name_end].to_string();
                let value = inner[val_start..val_end.min(n)].to_string();
                if !key.is_empty() {
                    attrs.insert(key, value);
                }
                i = (val_end + 1).min(n);
                continue;
            }
        }
        i += 1;
    }
}

fn decode_xml_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}
