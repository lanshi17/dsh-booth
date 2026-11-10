//! The decider between the portal and the sub-agents.
//!
//! A caller hands it one jev `judge` call: a state, and questions. A choice question names
//! exhaustive criteria. The decider picks when the top probability is at least 0.7. Otherwise a
//! sub-agent's call goes to the portal, and the portal's call goes to the user.

use crate::api::Api;
use crate::provider;
use crate::service::Config;
use serde_json::{json, Value};
use std::io::Read;
use std::time::Duration;

const FLOOR: f64 = 0.7;
const BAND_LOW: f64 = 0.3;
const START: &str = "<!-- booth-decider -->";
const END: &str = "<!-- /booth-decider -->";
const CONTRACT: &str = "\
A choice is settled by `booth decide` before the portal or the user sees it. The call is one jev judge on stdin: `from` is `subagent` or `portal`, `state` is the situation, and `questions` maps an id to `{ \"type\": \"choice\", \"instructions\", \"criteria\" }`. Each criterion is one sentence of observable evidence. Labels are exhaustive and mutually exclusive, with one catch-all.

A result with `\"route\": \"chosen\"` is the answer. Use `answers` and do not pick again.

A result with `\"route\": \"escalate\"` was not confident enough (top probability under 0.7, an uncertain bool, or an error). A sub-agent reports that call to the portal with `orchestrator_report` status `blocked`, puts the unanswered judge call in `summary`, and sets no `human_action`. The portal calls `booth decide` again with `\"from\": \"portal\"`. Only when that call escalates to `user` does the portal ask the user, once, through `human_action`. The user is the fallback after both calls come back unrouted.";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum From {
    Portal,
    Subagent,
}

impl From {
    fn parse(name: &str) -> Result<Self, String> {
        match name.trim().to_ascii_lowercase().as_str() {
            "portal" | "orchestrator" => Ok(Self::Portal),
            "subagent" | "worker" => Ok(Self::Subagent),
            other => Err(format!("from must be portal or subagent, not {other}")),
        }
    }

    fn escalate(self) -> &'static str {
        match self {
            Self::Subagent => "portal",
            Self::Portal => "user",
        }
    }
}

/// Read one judge call and print the route.
pub fn decide(cfg: &Config, input: &mut dyn Read) -> Result<(), String> {
    let mut text = String::new();
    input
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read the judge call: {error}"))?;
    if text.trim().is_empty() {
        return Err("booth decide reads one jev judge call on stdin".into());
    }
    let call: Value = serde_json::from_str(&text)
        .map_err(|error| format!("the judge call is not JSON: {error}"))?;
    let route = model_route(cfg)?;
    let verdict = ask(&call, &|prompt| complete(&route, prompt))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&verdict).unwrap_or_default()
    );
    Ok(())
}

/// Put the decider's order into the team prompt, once. A later call with the same text does nothing.
pub fn install(api: &Api) -> Result<(), String> {
    let view = api
        .get("/prompts")
        .map_err(|error| format!("cannot read prompts: {error}"))?;
    let current = view
        .get("current")
        .ok_or("the prompt response has no current revision")?;
    let revision = current
        .get("revision")
        .and_then(Value::as_u64)
        .ok_or("the prompt revision is missing")?;
    let config = current
        .get("config")
        .cloned()
        .ok_or("the prompt response has no config")?;
    let communication = config
        .get("communication")
        .and_then(Value::as_str)
        .ok_or("the team prompt has no communication section")?;
    let updated = with_contract(communication);
    if updated == communication {
        return Ok(());
    }
    let mut next = json!({
        "communication": updated,
        "scope": config.get("scope").cloned().unwrap_or(Value::String(String::new())),
        "delivery": config.get("delivery").cloned().unwrap_or(Value::String(String::new())),
        "skills": config.get("skills").cloned().unwrap_or(json!({})),
    });
    if let Some(overrides) = config.get("overrides") {
        next["overrides"] = overrides.clone();
    }
    api.put(
        "/prompts",
        &json!({ "config": next, "baseRevision": revision }),
    )
    .map(|_| ())
    .map_err(|error| format!("cannot save the decider prompt: {error}"))
}

/// The communication section with one decider block. Repeating it replaces that block.
pub fn with_contract(communication: &str) -> String {
    let block = format!("{START}\n{CONTRACT}\n{END}");
    let (head, tail) = match (communication.find(START), communication.find(END)) {
        (Some(start), Some(end)) if end > start => {
            let after = end + END.len();
            (&communication[..start], &communication[after..])
        }
        _ => (communication, ""),
    };
    let mut out = head.trim_end().to_owned();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(&block);
    let tail = tail.trim();
    if !tail.is_empty() {
        out.push_str("\n\n");
        out.push_str(tail);
    }
    out.push('\n');
    out
}

fn ask(call: &Value, model: &dyn Fn(&str) -> Result<String, String>) -> Result<Value, String> {
    let from = From::parse(call.get("from").and_then(Value::as_str).unwrap_or(""))?;
    validate(call)?;
    let answered = match model(&prompt(call)) {
        Ok(text) => match json_object(&text) {
            Ok(value) => answers_of(value),
            Err(error) => return Ok(escalate(from, &error, None)),
        },
        Err(error) => return Ok(escalate(from, &error, None)),
    };
    Ok(settle(from, call, &answered))
}

fn validate(call: &Value) -> Result<(), String> {
    let questions = call
        .get("questions")
        .and_then(Value::as_object)
        .filter(|questions| !questions.is_empty())
        .ok_or("a judge call needs one or more questions")?;
    if call.get("state").is_none() {
        return Err("a judge call needs a state".into());
    }
    for (id, question) in questions {
        let kind = question.get("type").and_then(Value::as_str).unwrap_or("");
        let instructions = question
            .get("instructions")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        if instructions.is_empty() {
            return Err(format!("question {id} needs instructions"));
        }
        match kind {
            "choice" | "score" => {
                let criteria = question
                    .get("criteria")
                    .and_then(Value::as_object)
                    .filter(|criteria| !criteria.is_empty())
                    .ok_or_else(|| format!("question {id} needs criteria"))?;
                if criteria.keys().any(|label| label.trim().is_empty()) {
                    return Err(format!("question {id} has an empty criterion"));
                }
            }
            "bool" => {}
            other => {
                return Err(format!(
                    "question {id} has type {other}; use choice, bool, or score"
                ))
            }
        }
    }
    Ok(())
}

fn prompt(call: &Value) -> String {
    format!(
        "You are the decider. Classify only. Reply with one JSON object and no other text.\n\
         Map each question id to an answer. A choice or score answer is \
         {{\"choice\":\"<label>\",\"probabilities\":{{\"<label>\":0}},\"confidence\":0}} \
         or {{\"score\":\"<label>\",\"probabilities\":{{\"<label>\":0}},\"confidence\":0}}. \
         Probabilities cover every criterion and sum to 1. \
         A bool answer is {{\"bool\":0}} as P(yes). Use only labels from the criteria.\n\n\
         STATE:\n{}\n\nQUESTIONS:\n{}",
        call.get("state").unwrap_or(&Value::Null),
        call.get("questions").unwrap_or(&Value::Null)
    )
}

fn settle(from: From, call: &Value, answers: &Value) -> Value {
    let Some(questions) = call.get("questions").and_then(Value::as_object) else {
        return escalate(from, "a judge call needs questions", None);
    };
    let Some(given) = answers.as_object() else {
        return escalate(from, "the decider returned no answers", None);
    };
    for (id, question) in questions {
        let Some(answer) = given.get(id) else {
            return escalate(
                from,
                &format!("question {id} has no answer"),
                Some(answers.clone()),
            );
        };
        if let Some(reason) = uncertain(id, question, answer) {
            return escalate(from, &reason, Some(answers.clone()));
        }
    }
    json!({ "route": "chosen", "answers": answers })
}

fn uncertain(id: &str, question: &Value, answer: &Value) -> Option<String> {
    match question.get("type").and_then(Value::as_str).unwrap_or("") {
        "bool" => {
            let Some(probability) = answer.get("bool").and_then(Value::as_f64) else {
                return Some(format!("question {id} has no probability"));
            };
            if (BAND_LOW..=FLOOR).contains(&probability) {
                Some(format!(
                    "question {id} is uncertain (P(yes)={probability:.2})"
                ))
            } else {
                None
            }
        }
        "choice" | "score" => {
            let label_key = if question.get("type").and_then(Value::as_str) == Some("score") {
                "score"
            } else {
                "choice"
            };
            let label = answer.get(label_key).and_then(Value::as_str).unwrap_or("");
            let Some(criteria) = question.get("criteria").and_then(Value::as_object) else {
                return Some(format!("question {id} needs criteria"));
            };
            if !criteria.contains_key(label) {
                return Some(format!(
                    "question {id} chose {label}, which is not a criterion"
                ));
            }
            let top = answer
                .get("probabilities")
                .and_then(Value::as_object)
                .and_then(|map| map.get(label))
                .and_then(Value::as_f64)
                .or_else(|| answer.get("confidence").and_then(Value::as_f64));
            match top {
                Some(probability) if probability >= FLOOR => None,
                Some(probability) => Some(format!(
                    "question {id} is uncertain (top probability {probability:.2} is below {FLOOR})"
                )),
                None => Some(format!("question {id} has no probability")),
            }
        }
        _ => Some(format!("question {id} cannot be routed")),
    }
}

fn escalate(from: From, reason: &str, answers: Option<Value>) -> Value {
    json!({
        "route": "escalate",
        "to": from.escalate(),
        "reason": reason,
        "answers": answers.unwrap_or(Value::Null),
    })
}

fn answers_of(value: Value) -> Value {
    value.get("answers").cloned().unwrap_or(value)
}

fn json_object(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|rest| rest.trim().trim_end_matches('`').trim())
        .unwrap_or(trimmed);
    if let Ok(value) = serde_json::from_str::<Value>(stripped) {
        if value.is_object() {
            return Ok(value);
        }
    }
    let start = stripped
        .find('{')
        .ok_or("the decider's answer was not a judge result")?;
    let mut depth = 0;
    for (index, ch) in stripped[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let object = &stripped[start..start + index + 1];
                    return serde_json::from_str(object).map_err(|error| {
                        format!("the decider's answer was not a judge result: {error}")
                    });
                }
            }
            _ => {}
        }
    }
    Err("the decider's answer was not a judge result".into())
}

struct ModelRoute {
    base: String,
    model: String,
    key: String,
}

fn model_route(cfg: &Config) -> Result<ModelRoute, String> {
    let text = std::fs::read_to_string(cfg.patch())
        .map_err(|error| format!("cannot read {}: {error}", cfg.patch().display()))?;
    let patch: Value = serde_norway::from_str(&text)
        .map_err(|error| format!("cannot read the profile: {error}"))?;
    route_from(&patch, provider::credential)
}

fn route_from(
    patch: &Value,
    secret: impl Fn(&str) -> Result<String, String>,
) -> Result<ModelRoute, String> {
    let entries = patch.as_array().ok_or("the profile patch is not a list")?;
    let entry = |id: &str| {
        entries
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
    };
    let default = entry("agent-default-model")
        .and_then(|item| item.get("config"))
        .ok_or("the profile has no default model")?;
    let provider_id = default
        .get("provider")
        .and_then(Value::as_str)
        .ok_or("the default model has no provider")?;
    let model = default
        .get("model")
        .and_then(Value::as_str)
        .ok_or("the default model has no id")?
        .to_owned();
    let provider = entry("llm-pi-ai")
        .and_then(|item| item.get("config"))
        .and_then(|config| config.get("providers"))
        .and_then(|providers| providers.get(provider_id))
        .ok_or_else(|| format!("the profile has no provider {provider_id}"))?;
    let api = provider.get("api").and_then(Value::as_str).unwrap_or("");
    if api != "openai-responses" {
        return Err(format!(
            "the decider calls {provider_id} through openai-responses; this profile uses {api}"
        ));
    }
    let base = provider
        .get("baseURL")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("provider {provider_id} has no base URL"))?
        .trim_end_matches('/')
        .to_owned();
    let key_ref = provider
        .get("apiKeyEnv")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("provider {provider_id} has no credential ref"))?;
    let key = std::env::var(key_ref)
        .ok()
        .filter(|value| !value.is_empty())
        .map(Ok)
        .unwrap_or_else(|| secret(key_ref))?;
    Ok(ModelRoute { base, model, key })
}

fn complete(route: &ModelRoute, prompt: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .build();
    let response = agent
        .post(&format!("{}/responses", route.base))
        .set("Authorization", &format!("Bearer {}", route.key))
        .set("User-Agent", "dsh/0.2.1")
        .set("accept", "application/json")
        .send_json(json!({
            "model": route.model,
            "input": prompt,
            "temperature": 0
        }));
    let body = match response {
        Ok(response) => response
            .into_string()
            .map_err(|error| format!("the decider's model returned no body: {error}"))?,
        Err(ureq::Error::Status(code, response)) => {
            let detail = response.into_string().unwrap_or_default();
            return Err(format!(
                "the decider's model returned HTTP {code}: {}",
                redact(&detail, &route.key)
            ));
        }
        Err(error) => return Err(format!("the decider cannot reach its model: {error}")),
    };
    let value: Value = serde_json::from_str(&body)
        .map_err(|error| format!("the decider's model returned no JSON: {error}"))?;
    output_text(&value).ok_or_else(|| "the decider's model returned no text".into())
}

fn output_text(value: &Value) -> Option<String> {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        let text = text.trim();
        if !text.is_empty() {
            return Some(text.to_owned());
        }
    }
    let mut parts = Vec::new();
    for item in value.get("output").and_then(Value::as_array)? {
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for block in content {
            if let Some(text) = block.get("text").and_then(Value::as_str) {
                parts.push(text);
            }
        }
    }
    let text = parts.concat();
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

fn redact(text: &str, secret: &str) -> String {
    let shown = if secret.is_empty() {
        text.to_owned()
    } else {
        text.replace(secret, "[redacted]")
    };
    let mut short = shown.chars().take(300).collect::<String>();
    if shown.chars().count() > 300 {
        short.push('…');
    }
    short
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(from: &str) -> Value {
        json!({
            "from": from,
            "state": {"request": "fix the readme"},
            "questions": {
                "route": {
                    "type": "choice",
                    "instructions": "Who does this change?",
                    "criteria": {
                        "dsh": "A repository change for the dsh team.",
                        "cursor-agent": "A change the person named cursor-agent for.",
                        "other": "Neither of those."
                    }
                }
            }
        })
    }

    fn chosen(label: &str, probability: f64) -> String {
        json!({
            "route": {"choice": label, "probabilities": {"dsh": probability, "cursor-agent": 1.0 - probability, "other": 0.0}, "confidence": probability}
        })
        .to_string()
    }

    #[test]
    fn a_confident_choice_is_the_route() {
        let verdict = ask(&call("subagent"), &|_| Ok(chosen("dsh", 0.91))).unwrap();
        assert_eq!(verdict["route"], "chosen");
        assert_eq!(verdict["answers"]["route"]["choice"], "dsh");
    }

    #[test]
    fn an_uncertain_subagent_goes_to_the_portal() {
        let verdict = ask(&call("worker"), &|_| Ok(chosen("dsh", 0.42))).unwrap();
        assert_eq!(verdict["route"], "escalate");
        assert_eq!(verdict["to"], "portal");
    }

    #[test]
    fn an_uncertain_portal_goes_to_the_user() {
        let verdict = ask(&call("portal"), &|_| Ok(chosen("dsh", 0.42))).unwrap();
        assert_eq!(verdict["to"], "user");
    }

    #[test]
    fn a_model_failure_escalates_instead_of_guessing() {
        let verdict = ask(&call("subagent"), &|_| Err("offline".into())).unwrap();
        assert_eq!(verdict["to"], "portal");
        assert!(verdict["reason"].as_str().unwrap().contains("offline"));
    }

    #[test]
    fn an_uncertain_bool_does_not_auto_route() {
        let call = json!({
            "from": "portal",
            "state": "the button label",
            "questions": {"keep": {"type": "bool", "instructions": "Keep the current label?"}}
        });
        let verdict = ask(&call, &|_| Ok(r#"{"keep":{"bool":0.5}}"#.into())).unwrap();
        assert_eq!(verdict["to"], "user");
    }

    #[test]
    fn the_contract_is_installed_once() {
        let once = with_contract("Keep progress internal.");
        let twice = with_contract(&once);
        assert_eq!(once, twice);
        assert!(once.contains("booth decide"));
        assert!(once.contains("Keep progress internal."));
    }

    #[test]
    fn the_route_comes_from_the_default_model() {
        let patch = serde_norway::from_str::<Value>(
            "
- id: llm-pi-ai
  config:
    providers:
      commandcode:
        api: openai-responses
        apiKeyEnv: COMMANDCODE_API_KEY
        baseURL: https://example.test/v1/
- id: agent-default-model
  config:
    provider: commandcode
    model: deepseek/deepseek-v4.1-flash
",
        )
        .unwrap();
        let route = route_from(&patch, |_| Ok("secret".into())).unwrap();
        assert_eq!(route.base, "https://example.test/v1");
        assert_eq!(route.model, "deepseek/deepseek-v4.1-flash");
        assert_eq!(route.key, "secret");
    }
}
