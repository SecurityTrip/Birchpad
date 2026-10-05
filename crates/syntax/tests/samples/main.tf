# The demo bucket.
variable "region" {
  type    = string
  default = "eu-west-1"
}

resource "aws_s3_bucket" "demo" {
  bucket = "demo-${var.region}"
  tags = {
    Name    = "Demo"
    Enabled = true
  }
}

output "bucket" {
  value = upper(aws_s3_bucket.demo.id)
}
