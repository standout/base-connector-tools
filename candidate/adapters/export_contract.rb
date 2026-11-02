# frozen_string_literal: true

# Trusted local test adapter only. This executes the selected WASM through AppBridge.
# Never run it in a web request or against an untrusted upload / customer connection.
require 'json'
require 'app_bridge'
wasm, config_path, output = ARGV
config = JSON.parse(File.read(config_path, encoding: 'UTF-8'))
require File.expand_path(config.fetch('test_helper'))
app = AppBridge::App.new(File.expand_path(wasm), environment_variables: {})
connection = AppBridge::Connection.new('synthetic-contract', 'Synthetic contract', TestHelper.base_connection.to_json)
operations = []
%w[action trigger].each do |kind|
  config.fetch("#{kind}s").each do |name|
    context = if kind == 'action'
                AppBridge::ActionContext.new(name, connection, '{}')
              else
                AppBridge::TriggerContext.new(name, connection, '{}', '{}')
              end
    operations << {
      kind: kind, name: name,
      input_schema: JSON.parse(app.public_send("#{kind}_input_schema", context)),
      output_schema: JSON.parse(app.public_send("#{kind}_output_schema", context))
    }
  end
end
File.write(output, JSON.pretty_generate({wit: app.wit_version, scope: 'partial', operations: operations}) + "\n")
