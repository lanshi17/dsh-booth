#include "interaction/approvals.hpp"

#include <filesystem>
#include <string>

#include <catch2/catch_test_macros.hpp>

TEST_CASE("remembered approvals match the exact request and clear") {
  const auto path = std::filesystem::temp_directory_path() / "booth-approvals-test.json";
  std::filesystem::remove(path);
  booth::PermissionRequest request;
  request.tool = "bash";
  request.title = "bash";
  request.detail = "{\"command\":\"ls\"}";
  request.options.push_back(booth::PermissionOption{"allow", "Allow once", "allow_once"});
  request.options.push_back(booth::PermissionOption{"always", "Allow always", "allow_always"});
  std::string error;
  REQUIRE(booth::remember_approval(path.string(), request, error));
  REQUIRE(booth::remembered_approval(path.string(), request) == "always");
  REQUIRE(booth::clear_approvals(path.string(), error));
  REQUIRE_FALSE(booth::remembered_approval(path.string(), request).has_value());

  booth::PermissionRequest other = request;
  other.detail = "{\"command\":\"pwd\"}";
  REQUIRE(booth::remember_approval(path.string(), request, error));
  REQUIRE_FALSE(booth::remembered_approval(path.string(), other).has_value());
  REQUIRE(booth::clear_approvals(path.string(), error));
}
