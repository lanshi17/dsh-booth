#pragma once

#include <expected>
#include <optional>
#include <string>
#include <string_view>

namespace booth {

/// Which layer produced a failure. One problem per layer, never one per row.
enum class Layer { Config, State, Transport, Protocol, Process, Kernel };

struct Error {
  Layer layer{Layer::State};
  std::string detail;
  std::optional<int> status;  // set when the layer is a protocol/HTTP status
};

template <class T>
using Result = std::expected<T, Error>;

inline std::string_view layer_name(Layer layer) {
  switch (layer) {
    case Layer::Config: return "config";
    case Layer::State: return "state";
    case Layer::Transport: return "transport";
    case Layer::Protocol: return "protocol";
    case Layer::Process: return "process";
    case Layer::Kernel: return "kernel";
  }
  return "unknown";
}

inline std::string to_string(const Error& error) {
  std::string out{layer_name(error.layer)};
  out += ": ";
  out += error.detail;
  if (error.status) {
    out += " (status ";
    out += std::to_string(*error.status);
    out += ")";
  }
  return out;
}

}  // namespace booth
