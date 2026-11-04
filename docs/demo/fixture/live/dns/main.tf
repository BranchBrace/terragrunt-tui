resource "terraform_data" "record" {
  input = "dns-record"
}

output "id" {
  value = "dns"
}
