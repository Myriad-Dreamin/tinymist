#let bucket-by(data, key-col) = {
  let buckets = (:)
  let order = ()
  for row in data {
    let key = str(row.at(key-col, default: ""))
    if key == "" { continue }
    if key in buckets {
      let bucket = buckets.at(key)
      bucket.push(row)
      buckets.insert(key, bucket)
    } else {
      buckets.insert(key, (row,))
      order.push(key)
    }
  }
  (buckets: buckets, order: order)
}

