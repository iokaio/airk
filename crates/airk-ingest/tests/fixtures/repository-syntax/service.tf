# SPDX-License-Identifier: Apache-2.0
variable "deployment" { default = "mountain" }
provider "fictional" { region = "local" }
resource "fictional_service" "café" {
  name = var.deployment
  depends_on = [fictional_service.upstream]
  metadata { note = "var.not_a_reference" }
}
data "fictional_service" "existing" { name = "archive" }
module "worker" { source = "./worker" }
output "service_name" { value = fictional_service.café.name }
