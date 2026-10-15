# SPDX-License-Identifier: Apache-2.0
require "fictional/archive"
require_relative "local_reader"
module Named
  def label
    "café"
  end
end
class Archive
  include Named
  extend LocalHelpers
  def read
    reader.open
    reader&.close
    self.send(method_name)
  end
end
