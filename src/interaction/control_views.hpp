#pragma once

#include <string>

#include <nlohmann/json.hpp>

namespace booth {

/// Rendered control replies shared by the `/tree`, `/jobs`, `/tools`, `/dirs`
/// and `/agents` commands. Kept out of the dispatcher so command flows stay small.
std::string tree_text(const nlohmann::json& result);
std::string jobs_text(const nlohmann::json& result, bool full = false);
std::string tools_text(const nlohmann::json& result);
std::string dirs_text(const nlohmann::json& result);
std::string agents_text(const nlohmann::json& result);

}  // namespace booth
