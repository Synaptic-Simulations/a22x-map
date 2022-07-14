## map-server

The map server serves `png` images of the rendered map. 

URL format:

```
http://127.0.0.1/map.png?
```

where the query parameters are:

* `id={}`: The id of the map to render.
* `res={},{}`: The resolution of the image - width, then height.
* `pos={},{}`: The position of the center - latitude, then longitude.
* `heading={}`: The heading of the map in degrees.
* `range={}`: The vertical range of the map in radians.
* `alt={}`: The altitude of the aircraft in feet MSL.
