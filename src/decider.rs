//! The decider between the portal and the sub-agents.
//!
//! A caller hands it one System One request for `typesafe/jev`: a state, and typed questions.
//! The model returns probabilities, never text. A choice or score is taken when its confidence
//! is at least 0.7, and a noul when yes or no is clear. Otherwise a sub-agent's call goes to the
//! portal, and the portal's call goes to the user.

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
const MODEL: &str = "typesafe/jev";
const PORTAL_CONTRACT: &str = r#"For the main orchestrator Session handling booth local console, WeChat iLink or Feishu messages, act as the user's persistent team lead. These local, WeChat and Feishu rules override the routing and reply instructions that forbid planning or conversation; other Sources retain their existing rules. Worker Sessions do not act as the portal and must follow their worker instructions.

Every local, WeChat or Feishu candidate is a user conversation turn. Consider all messages in a candidate batch, including earlier messages, before deciding. Call booth_portal_conversation with its trigger_id to obtain the durable conversation id and task list. Reply naturally using booth_portal_reply with that conversation id; greetings, questions, clarifications, progress and final results all deserve a useful response. Finish each local, WeChat or Feishu candidate with orchestrator_route kind ignore and a short reason such as 'handled by portal' (unless using that trigger to stop a ticket). Never accept a local, WeChat or Feishu conversation message directly as a worker ticket: that would send subsequent conversation to the worker instead of you.

For a concrete goal, identify independent work and plan a separate task for each using booth_portal_plan; for a new conversation its conversation_id is the current local, WeChat or Feishu trigger_id, and its returned conversation_id is the stable identifier. Task ids must be unique within the conversation. Include necessary context and scope in each instruction, without inventing user requirements. Set depends_on to task ids that must finish first. Dispatch ready tasks using booth_portal_dispatch, with a meaningful lowercase worktree_name. Dispatch independent tasks without waiting for one another. Do not delegate a simple conversation or a question requiring no investigation.

A worker_report or worker_attention with a portal field identifies conversationId and taskId. Read booth_portal_tasks for the current state; when prerequisites complete, dispatch newly ready tasks. Reconcile worker findings and report accurate progress, blockers and results to the user using booth_portal_reply. Ask the user only when needed for a decision or missing information, and do not claim work is complete until all required tasks finish. Use booth_portal_forward with the current user trigger to deliver a clarification or changed instruction to the relevant worker; use orchestrator_ticket_note to share findings between workers; use the person's explicit stop trigger with orchestrator_route kind stop to stop selected worker tickets. Preserve all explicit user constraints. Do not wait or poll: worker reports and new messages arrive as events.
"#;
const CONTRACT: &str = "\
A choice is settled by `booth decide` before the portal or the user sees it. The call on stdin is one System One request for typesafe/jev, plus `from`. `from` is `subagent` or `portal` and is not sent to the model. `state` is shared by every question. A question is `noul` (is this true; optional criteria `{\"true\",\"false\"}`; the answer field `noul` is the probability of yes), `choice` (criteria is an object from option to description; the answer has `choice`, `confidence`, and `probabilities`), or `score` (criteria is an array of at least two levels, lowest first; the answer has `score`, `confidence`, `legend`, and `probabilities`). One call holds at most 100 questions.

A result with `\"route\": \"chosen\"` is the answer. Use `answers` and do not pick again.

A result with `\"route\": \"escalate\"` was not confident enough (a choice or score under 0.7, a noul between 0.3 and 0.7, or an error). A sub-agent reports that call to the portal with `orchestrator_report` status `blocked`, puts the unanswered judge call in `summary`, and sets no `human_action`. The portal calls `booth decide` again with `\"from\": \"portal\"`. Only when that call escalates to `user` does the portal ask the user, once, through `human_action`. The user is the fallback after both calls come back unrouted.";

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
///
/// `provider` and `model` override the saved decider route for this call. With neither, booth
/// uses the profile's `decider-model` entry, and otherwise commandcode and `typesafe/jev`.
pub fn decide(
    cfg: &Config,
    input: &mut dyn Read,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<(), String> {
    let mut text = String::new();
    input
        .read_to_string(&mut text)
        .map_err(|error| format!("cannot read the judge call: {error}"))?;
    if text.trim().is_empty() {
        return Err("booth decide reads one jev judge call on stdin".into());
    }
    let call: Value = serde_json::from_str(&text)
        .map_err(|error| format!("the judge call is not JSON: {error}"))?;
    let route = model_route(cfg, provider, model)?;
    let verdict = ask(&call, &route.model, &mut |body| complete(&route, body))?;
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
    let block = format!("{START}\n{CONTRACT}\n\n{PORTAL_CONTRACT}\n{END}");
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

/// Remember which provider and model later decisions call.
pub fn save_route(cfg: &Config, provider: &str, model: &str) -> Result<(), String> {
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty() || model.is_empty() {
        return Err("--save needs --provider and --model".into());
    }
    model_route(cfg, Some(provider), Some(model))?;
    let mut patch = crate::service::Patch::read(cfg)?;
    patch.set_config(
        "decider-model",
        json!({ "provider": provider, "model": model }),
    );
    patch.write(cfg)?;
    println!("decider route {provider}/{model}");
    Ok(())
}

fn ask(
    call: &Value,
    model_name: &str,
    model: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<Value, String> {
    let from = From::parse(call.get("from").and_then(Value::as_str).unwrap_or(""))?;
    validate(call)?;
    let answered = match model(&systemone_body(call, model_name)) {
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
    if questions.len() > 100 {
        return Err("a judge call takes at most 100 questions".into());
    }
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
            "noul" => {
                if let Some(criteria) = question.get("criteria") {
                    if !criteria.is_object() {
                        return Err(format!(
                            "question {id} has noul criteria that are not {{\"true\",\"false\"}}"
                        ));
                    }
                }
            }
            "choice" => {
                let criteria = question
                    .get("criteria")
                    .and_then(Value::as_object)
                    .filter(|criteria| !criteria.is_empty())
                    .ok_or_else(|| format!("question {id} needs criteria"))?;
                if criteria.iter().any(|(label, text)| {
                    label.trim().is_empty() || text.as_str().unwrap_or("").trim().is_empty()
                }) {
                    return Err(format!("question {id} has an empty criterion"));
                }
            }
            "score" => {
                let levels = question
                    .get("criteria")
                    .and_then(Value::as_array)
                    .filter(|levels| levels.len() >= 2)
                    .ok_or_else(|| {
                        format!("question {id} needs a score rubric of at least two levels, lowest first")
                    })?;
                if levels
                    .iter()
                    .any(|level| level.as_str().unwrap_or("").trim().is_empty())
                {
                    return Err(format!("question {id} has an empty score level"));
                }
            }
            other => {
                return Err(format!(
                    "question {id} has type {other}; use noul, choice, or score"
                ))
            }
        }
    }
    Ok(())
}

/// The System One body. `from` stays with booth and is not part of the model request.
fn systemone_body(call: &Value, model: &str) -> String {
    json!({
        "model": model,
        "state": call.get("state").cloned().unwrap_or(Value::Null),
        "questions": call.get("questions").cloned().unwrap_or(json!({})),
    })
    .to_string()
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
        "noul" => {
            let Some(probability) = answer.get("noul").and_then(Value::as_f64) else {
                return Some(format!("question {id} has no probability"));
            };
            if probability > BAND_LOW && probability < FLOOR {
                Some(format!(
                    "question {id} is uncertain (P(yes)={probability:.2})"
                ))
            } else if (0.0..=1.0).contains(&probability) {
                None
            } else {
                Some(format!("question {id} has P(yes)={probability:.2}"))
            }
        }
        "choice" => {
            let label = answer.get("choice").and_then(Value::as_str).unwrap_or("");
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
        "score" => {
            let Some(levels) = question.get("criteria").and_then(Value::as_array) else {
                return Some(format!("question {id} needs a score rubric"));
            };
            let Some(confidence) = answer.get("confidence").and_then(Value::as_f64) else {
                return Some(format!("question {id} has no confidence"));
            };
            if confidence < FLOOR {
                return Some(format!(
                    "question {id} is uncertain (confidence {confidence:.2} is below {FLOOR})"
                ));
            }
            let Some(score) = answer.get("score").and_then(Value::as_f64) else {
                return Some(format!("question {id} has no score"));
            };
            let last = (levels.len() - 1) as f64;
            if (0.0..=last).contains(&score) {
                None
            } else {
                Some(format!("question {id} scored {score}, outside the rubric"))
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
    path: String,
    model: String,
    key: String,
}

struct RouteChoice<'a> {
    provider: Option<&'a str>,
    model: Option<&'a str>,
}

fn model_route(
    cfg: &Config,
    provider: Option<&str>,
    model: Option<&str>,
) -> Result<ModelRoute, String> {
    let text = std::fs::read_to_string(cfg.patch())
        .map_err(|error| format!("cannot read {}: {error}", cfg.patch().display()))?;
    let patch: Value = serde_norway::from_str(&text)
        .map_err(|error| format!("cannot read the profile: {error}"))?;
    route_from(
        &patch,
        RouteChoice { provider, model },
        provider::credential,
    )
}

fn named(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|text| !text.is_empty())
}

fn route_from(
    patch: &Value,
    choice: RouteChoice<'_>,
    secret: impl Fn(&str) -> Result<String, String>,
) -> Result<ModelRoute, String> {
    let entries = patch.as_array().ok_or("the profile patch is not a list")?;
    let entry = |id: &str| {
        entries
            .iter()
            .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
    };
    let saved = entry("decider-model").and_then(|item| item.get("config"));
    let saved_str = |key: &str| {
        saved
            .and_then(|config| config.get(key))
            .and_then(Value::as_str)
    };
    let provider_id = named(choice.provider)
        .or_else(|| named(saved_str("provider")))
        .unwrap_or("commandcode")
        .to_owned();
    let model = named(choice.model)
        .or_else(|| named(saved_str("model")))
        .unwrap_or(MODEL)
        .to_owned();
    let path = endpoint_path(named(saved_str("path")).unwrap_or("/systemone"));
    let provider = entry("llm-pi-ai")
        .and_then(|item| item.get("config"))
        .and_then(|config| config.get("providers"))
        .and_then(|providers| providers.get(&provider_id))
        .ok_or_else(|| format!("the profile has no provider {provider_id}"))?;
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
    Ok(ModelRoute {
        base,
        path,
        model,
        key,
    })
}

fn endpoint_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

fn complete(route: &ModelRoute, body: &str) -> Result<String, String> {
    let payload: Value = serde_json::from_str(body)
        .map_err(|error| format!("the System One request is not JSON: {error}"))?;
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(60))
        .build();
    let response = agent
        .post(&format!("{}{}", route.base, route.path))
        .set("Authorization", &format!("Bearer {}", route.key))
        .set("User-Agent", "dsh/0.2.1")
        .set("accept", "application/json")
        .send_json(payload);
    let text = match response {
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
    let value: Value = serde_json::from_str(&text)
        .map_err(|error| format!("the decider's model returned no JSON: {error}"))?;
    if value.get("answers").is_none() {
        return Err("the decider's model returned no answers".into());
    }
    Ok(text)
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
        let verdict = ask(&call("subagent"), MODEL, &mut |_| Ok(chosen("dsh", 0.91))).unwrap();
        assert_eq!(verdict["route"], "chosen");
        assert_eq!(verdict["answers"]["route"]["choice"], "dsh");
    }

    #[test]
    fn an_uncertain_subagent_goes_to_the_portal() {
        let verdict = ask(&call("worker"), MODEL, &mut |_| Ok(chosen("dsh", 0.42))).unwrap();
        assert_eq!(verdict["route"], "escalate");
        assert_eq!(verdict["to"], "portal");
    }

    #[test]
    fn an_uncertain_portal_goes_to_the_user() {
        let verdict = ask(&call("portal"), MODEL, &mut |_| Ok(chosen("dsh", 0.42))).unwrap();
        assert_eq!(verdict["to"], "user");
    }

    #[test]
    fn a_model_failure_escalates_instead_of_guessing() {
        let verdict = ask(&call("subagent"), MODEL, &mut |_| Err("offline".into())).unwrap();
        assert_eq!(verdict["to"], "portal");
        assert!(verdict["reason"].as_str().unwrap().contains("offline"));
    }

    #[test]
    fn a_clear_noul_is_the_route_and_an_uncertain_one_is_not() {
        let call = json!({
            "from": "portal",
            "state": "My payments have failed for three days and I am losing sales.",
            "questions": {"is_urgent": {"type": "noul", "instructions": "Does this request need urgent attention?"}}
        });
        let clear = ask(&call, MODEL, &mut |_| {
            Ok(r#"{"answers":{"is_urgent":{"type":"noul","noul":0.96}}}"#.into())
        })
        .unwrap();
        assert_eq!(clear["route"], "chosen");
        assert_eq!(clear["answers"]["is_urgent"]["noul"], 0.96);
        let unclear = ask(&call, MODEL, &mut |_| {
            Ok(r#"{"answers":{"is_urgent":{"type":"noul","noul":0.5}}}"#.into())
        })
        .unwrap();
        assert_eq!(unclear["to"], "user");
    }

    #[test]
    fn an_uncertain_score_does_not_auto_route() {
        let call = json!({
            "from": "subagent",
            "state": "Please help now.",
            "questions": {"frustration": {"type": "score", "instructions": "How frustrated is the customer?", "criteria": ["Calm", "Frustrated", "Very angry"]}}
        });
        let verdict = ask(&call, MODEL, &mut |_| {
            Ok(r#"{"answers":{"frustration":{"type":"score","score":1.41,"confidence":0.5,"legend":{"0":"Calm","1":"Frustrated","2":"Very angry"},"probabilities":{"0":0.02,"1":0.55,"2":0.43}}}}"#.into())
        })
        .unwrap();
        assert_eq!(verdict["to"], "portal");
    }

    #[test]
    fn the_model_receives_a_systemone_request() {
        let mut seen = String::new();
        let _ = ask(&call("portal"), "typesafe/jev-1.14.0", &mut |body| {
            seen = body.to_owned();
            Ok(chosen("dsh", 0.91))
        })
        .unwrap();
        let value: Value = serde_json::from_str(&seen).unwrap();
        assert_eq!(value["model"], "typesafe/jev-1.14.0");
        assert!(value.get("from").is_none());
        assert!(value.get("questions").is_some());
    }

    #[test]
    fn the_contract_is_installed_once() {
        let once = with_contract("Keep progress internal.");
        let twice = with_contract(&once);
        assert_eq!(once, twice);
        for behavior in [
            "persistent team lead",
            "WeChat iLink",
            "Feishu",
            "booth_portal_plan",
            "booth_portal_dispatch",
            "booth_portal_reply",
            "booth_portal_forward",
        ] {
            assert!(
                once.contains(behavior),
                "missing portal behavior {behavior}"
            );
        }
        assert!(once.contains("booth decide"));
        assert!(once.contains("Keep progress internal."));
    }

    #[test]
    fn the_decider_calls_commandcode_systemone() {
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
        let route = route_from(
            &patch,
            RouteChoice {
                provider: None,
                model: None,
            },
            |_| Ok("secret".into()),
        )
        .unwrap();
        assert_eq!(route.base, "https://example.test/v1");
        assert_eq!(route.model, "typesafe/jev");
        assert_eq!(route.path, "/systemone");
        assert_eq!(route.key, "secret");
    }

    #[test]
    fn a_saved_route_picks_the_provider_and_a_flag_overrides_it() {
        let patch = serde_norway::from_str::<Value>(
            "
- id: llm-pi-ai
  config:
    providers:
      commandcode:
        apiKeyEnv: COMMANDCODE_API_KEY
        baseURL: https://example.test/v1
      other:
        apiKeyEnv: OTHER_KEY
        baseURL: https://other.test/v1/
- id: decider-model
  config:
    provider: other
    model: typesafe/jev-1.14.0
    path: systemone
",
        )
        .unwrap();
        let saved = route_from(
            &patch,
            RouteChoice {
                provider: None,
                model: None,
            },
            |name| Ok(name.into()),
        )
        .unwrap();
        assert_eq!(saved.base, "https://other.test/v1");
        assert_eq!(saved.model, "typesafe/jev-1.14.0");
        assert_eq!(saved.path, "/systemone");
        assert_eq!(saved.key, "OTHER_KEY");
        let once = route_from(
            &patch,
            RouteChoice {
                provider: Some("commandcode"),
                model: Some("typesafe/jev"),
            },
            |name| Ok(name.into()),
        )
        .unwrap();
        assert_eq!(once.base, "https://example.test/v1");
        assert_eq!(once.model, "typesafe/jev");
        assert!(route_from(
            &patch,
            RouteChoice {
                provider: Some("missing"),
                model: None,
            },
            |_| Ok("secret".into()),
        )
        .is_err());
    }
}
